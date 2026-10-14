import importlib.util
import json
import os
import pathlib
import plistlib
import struct
import subprocess
import tempfile
import unittest
import zipfile
from unittest.mock import patch

ROOT=pathlib.Path(__file__).resolve().parents[2]
def module(name, path):
    spec=importlib.util.spec_from_file_location(name,ROOT/path)
    m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
v=module("validator","scripts/validate-ota-artifacts.py")
pub=module("publisher","scripts/publish-release.py")

def fixture(directory,target):
    m=v.meta.metadata(target);channel=m['channel'];version=m['version']
    (directory/f"build.{channel}.json").write_text(json.dumps(m))
    manifest=f'<package><metadata><id>{m["package_id"]}</id><version>{version}</version><channel>{channel}</channel><shortcutAumid>{m["package_id"]}</shortcutAumid></metadata></package>'
    files={}
    if target.startswith('x86_64-pc'):
        pe=bytearray(256);pe[:2]=b'MZ';struct.pack_into('<I',pe,0x3c,64);pe[64:68]=b'PE\0\0';struct.pack_into('<H',pe,68,0x8664)
        files={'lib/win/gulu-twist.exe':pe,'lib/win/Squirrel.exe':b'updater'}
        (directory/f"app-{channel}-Setup.exe").write_bytes(pe)
    else:
        cpu=0x100000c if target.startswith('aarch64') else 0x1000007
        files={'App.app/Contents/MacOS/gulu-twist':b'\xcf\xfa\xed\xfe'+struct.pack('<I',cpu),
               'App.app/Contents/MacOS/UpdateMac':b'updater','App.app/Contents/Resources/sq.version':manifest,
               'App.app/Contents/Info.plist':plistlib.dumps(dict(CFBundleIdentifier=m['package_id'],CFBundleShortVersionString=version,LSUIElement=True))}
        with zipfile.ZipFile(directory/f"app-{channel}-Portable.zip",'w') as z:
            for n,data in files.items():z.writestr(n,data)
    package=directory/f'app-{version}-{channel}-full.nupkg'
    with zipfile.ZipFile(package,'w') as z:
        z.writestr('app.nuspec',manifest)
        for n,data in files.items():z.writestr(n,data)
    feed=dict(Assets=[dict(FileName=package.name,Type='Full',Version=version,PackageId=m['package_id'],Size=package.stat().st_size,SHA256=v.sha256(package))])
    (directory/f'releases.{channel}.json').write_text(json.dumps(feed))
    return m,package

class Artifacts(unittest.TestCase):
    def test_complete_three_targets_and_missing_target(self):
        with tempfile.TemporaryDirectory() as d:
            directory=pathlib.Path(d)
            for target in v.meta.TARGETS:fixture(directory,target)
            self.assertEqual(len(v.validate(directory,v.meta.TARGETS)),12)
            (directory/'releases.win-x64-stable.json').unlink()
            with self.assertRaises(FileNotFoundError):v.validate(directory,v.meta.TARGETS)
    def test_wrong_digest_and_cross_commit_are_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            directory=pathlib.Path(d);target='aarch64-apple-darwin';_,package=fixture(directory,target)
            with self.assertRaises(ValueError):v.validate(directory,[target],'f'*40)
            package.write_bytes(package.read_bytes()+b'corrupt')
            with self.assertRaises(ValueError):v.validate(directory,[target])
    def test_path_escape_and_delta_are_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            directory=pathlib.Path(d);target='aarch64-apple-darwin';m,_=fixture(directory,target)
            path=directory/f"releases.{m['channel']}.json";feed=json.loads(path.read_text())
            feed['Assets'][0]['FileName']='../outside-full.nupkg';path.write_text(json.dumps(feed))
            with self.assertRaises(ValueError):v.validate(directory,[target])
    def test_different_package_architecture_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            directory=pathlib.Path(d);_,package=fixture(directory,'aarch64-apple-darwin')
            with self.assertRaises(ValueError):v.validate_archive(package,'x86_64-apple-darwin',v.meta.metadata('aarch64-apple-darwin')['version'])

class DraftPublishing(unittest.TestCase):
    def run_publish(self,release=None,api_error=False,ref=None):
        calls=[]
        with tempfile.TemporaryDirectory() as d:
            directory=pathlib.Path(d)
            for target in v.meta.TARGETS:fixture(directory,target)
            commit=v.meta.metadata('aarch64-apple-darwin')['commit']
            if release is not None:release=dict(tag_name='v'+v.meta.metadata('aarch64-apple-darwin')['version'],target_commitish=commit,assets=[],**release)
            def fake(*args,**kw):
                calls.append(args)
                if args[0]=='api':
                    if api_error:raise RuntimeError('HTTP 403')
                    if 'git/ref' in args[1]:return ref
                    return json.dumps([[release] if release else []])
                if args[:2]==('release','view'):return json.dumps(dict(isDraft=True,url='https://example.test/draft'))
                return ''
            with patch.dict(os.environ,{'GH_REPO':'test/repo','GITHUB_SHA':commit},clear=True),patch.object(pub,'gh',side_effect=fake):
                pub.main(directory)
        return calls
    def test_new_release_stays_draft(self):
        calls=self.run_publish()
        create=next(c for c in calls if c[:2]==('release','create'))
        self.assertIn('--draft',create)
        self.assertFalse(any('--draft=false' in c for c in calls))
    def test_empty_matching_draft_can_resume(self):
        calls=self.run_publish(dict(draft=True))
        self.assertFalse(any(c[:2]==('release','create') for c in calls))
        self.assertTrue(any(c[:2]==('release','upload') for c in calls))
    def test_interrupted_upload_only_resumes_original_artifacts(self):
        for changed in (False, True):
            with self.subTest(changed=changed), tempfile.TemporaryDirectory() as d:
                directory = pathlib.Path(d)
                for target in v.meta.TARGETS:
                    fixture(directory, target)
                metadata = v.meta.metadata('aarch64-apple-darwin')
                release = dict(tag_name='v'+metadata['version'], target_commitish=metadata['commit'], draft=True, assets=[])
                remote_manifest = None
                uploads = []
                interrupted = False

                def fake(*args, **kw):
                    nonlocal remote_manifest, interrupted
                    if args[0] == 'api':
                        return None if 'git/ref' in args[1] else json.dumps([[release]])
                    if args[:2] == ('release', 'download'):
                        destination = pathlib.Path(args[args.index('--dir')+1])
                        (destination/'release-manifest.json').write_text(remote_manifest)
                    if args[:2] == ('release', 'upload'):
                        uploads.append(args)
                        first = pathlib.Path(args[3])
                        if first.name == 'release-manifest.json':
                            remote_manifest = first.read_text()
                            release['assets'] = [dict(name=first.name)]
                        elif not interrupted:
                            interrupted = True
                            release['assets'].append(dict(name=first.name))
                            raise RuntimeError('connection interrupted during payload upload')
                    if args[:2] == ('release', 'view'):
                        return json.dumps(dict(isDraft=True, url='https://example.test/draft'))
                    return ''

                with patch.dict(os.environ, {'GH_REPO':'test/repo', 'GITHUB_SHA':metadata['commit']}, clear=True), patch.object(pub, 'gh', side_effect=fake):
                    with self.assertRaisesRegex(RuntimeError, 'interrupted'):
                        pub.main(directory)
                    self.assertEqual(pathlib.Path(uploads[0][3]).name, 'release-manifest.json')
                    count = len(uploads)
                    if changed:
                        with zipfile.ZipFile(next(directory.glob('*Portable.zip')), 'a') as z:
                            z.comment = b'different rebuild'
                        with self.assertRaisesRegex(ValueError, '构建内容不同'):
                            pub.main(directory)
                        self.assertEqual(len(uploads), count)
                    else:
                        pub.main(directory)
                        self.assertEqual(len(uploads), count+2)
    def test_published_release_is_immutable(self):
        with self.assertRaisesRegex(ValueError,'已经公开'):self.run_publish(dict(draft=False))
    def test_api_failure_cannot_create_release(self):
        with self.assertRaisesRegex(RuntimeError,'403'):self.run_publish(api_error=True)
    def test_tag_bound_to_different_commit_fails(self):
        with self.assertRaisesRegex(ValueError,'不同提交'):self.run_publish(ref=json.dumps(dict(object=dict(type='commit',sha='f'*40))))
    def test_only_real_404_is_missing(self):
        with patch.object(pub.subprocess,'run',return_value=subprocess.CompletedProcess([],1,'','gh: forbidden (HTTP 403)')):
            with self.assertRaises(RuntimeError):pub.gh('api','ref',optional_404=True)
        with patch.object(pub.subprocess,'run',return_value=subprocess.CompletedProcess([],1,'','gh: Not Found (HTTP 404)')):
            self.assertIsNone(pub.gh('api','ref',optional_404=True))

if __name__=='__main__':unittest.main()
