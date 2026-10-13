// Editable SVG masters -> antialiased application, tray and Windows resources.
import sharp from 'sharp';
import fs from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const icons = path.join(root, 'assets/icons');
async function render(source, n) {
  return sharp(path.join(icons, 'source', source), {density: 384}).resize(n,n).png().toBuffer();
}
await fs.mkdir(path.join(icons, 'sizes'), {recursive:true});
for (const n of [16,20,24,32,40,48,64,128,256,512,1024]) {
  const png = await render('app.svg',n);
  await fs.writeFile(path.join(icons,'sizes',`app-${n}.png`),png);
  if(n===1024) await fs.writeFile(path.join(icons,'app.png'),png);
}
await fs.writeFile(path.join(icons,'mascot.png'),await render('mascot.svg',512));
for(const n of [18,36]) await fs.writeFile(path.join(icons,`tray-template-${n}.png`),await render('tray.svg',n));
// Native tray API consumes premultiplied-free RGBA; commit the decoded bytes.
await fs.writeFile(path.join(icons,'tray-macos.rgba'),await sharp(await render('tray.svg',36)).ensureAlpha().raw().toBuffer());
await fs.writeFile(path.join(icons,'tray-windows.rgba'),await sharp(await render('app.svg',32)).ensureAlpha().raw().toBuffer());
const sizes = [16,20,24,32,40,48,64,128,256];
const images = await Promise.all(sizes.map(n=>render('app.svg',n)));
const header=Buffer.alloc(6+16*sizes.length); header.writeUInt16LE(1,2);header.writeUInt16LE(sizes.length,4);
let offset=header.length;
sizes.forEach((n,i)=>{const at=6+i*16;header[at]=n%256;header[at+1]=n%256;header.writeUInt16LE(1,at+4);header.writeUInt16LE(32,at+6);header.writeUInt32LE(images[i].length,at+8);header.writeUInt32LE(offset,at+12);offset+=images[i].length;});
await fs.writeFile(path.join(icons,'app.ico'),Buffer.concat([header,...images]));
// Keep the two alternative concepts as editable exploration, not production assets.
for(const c of ['concept-b','concept-c']) await fs.writeFile(path.join(icons,`${c}.png`),await render(`${c}.svg`,512));
console.log('Exported SVG masters, 11 PNG sizes, 9-size ICO and native tray RGBA.');
