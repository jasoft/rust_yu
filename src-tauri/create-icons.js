const fs = require('fs');
const path = require('path');
const sharp = require('sharp');

const iconsDir = path.join(__dirname, 'icons');

async function createIcons() {
    // 创建蓝色背景 + Y 字的 SVG
    const svgImage = `
    <svg width="256" height="256" xmlns="http://www.w3.org/2000/svg">
        <rect width="256" height="256" fill="#2563EB"/>
        <text x="128" y="180" font-family="Arial" font-size="180" font-weight="bold" fill="white" text-anchor="middle">Y</text>
    </svg>
    `;

    const svgBuffer = Buffer.from(svgImage);

    // 创建 32x32
    await sharp(svgBuffer)
        .resize(32, 32)
        .png()
        .toFile(path.join(iconsDir, '32x32.png'));

    // 创建 128x128
    await sharp(svgBuffer)
        .resize(128, 128)
        .png()
        .toFile(path.join(iconsDir, '128x128.png'));

    // 创建 128x128@2x (256x256)
    await sharp(svgBuffer)
        .resize(256, 256)
        .png()
        .toFile(path.join(iconsDir, '128x128@2x.png'));

    // 手动创建 ICO 文件 (简单的单尺寸 ICO)
    const pngData = fs.readFileSync(path.join(iconsDir, '32x32.png'));

    // ICO 文件头
    const iconDir = Buffer.alloc(6);
    iconDir.writeUInt16LE(0, 0);      // Reserved
    iconDir.writeUInt16LE(1, 2);      // Type: 1 = ICO
    iconDir.writeUInt16LE(1, 4);      // Number of images

    // ICO 目录项
    const iconDirEntry = Buffer.alloc(16);
    iconDirEntry.writeUInt8(32, 0);    // Width (0 = 256)
    iconDirEntry.writeUInt8(32, 1);    // Height
    iconDirEntry.writeUInt8(0, 2);     // Color palette
    iconDirEntry.writeUInt8(0, 3);     // Reserved
    iconDirEntry.writeUInt16LE(1, 4);  // Color planes
    iconDirEntry.writeUInt16LE(32, 6); // Bits per pixel
    iconDirEntry.writeUInt32LE(pngData.length, 8);  // Size of image data
    iconDirEntry.writeUInt32LE(22, 12); // Offset to image data (6 + 16 = 22)

    const icoBuffer = Buffer.concat([iconDir, iconDirEntry, pngData]);
    fs.writeFileSync(path.join(iconsDir, 'icon.ico'), icoBuffer);

    console.log('Icons created successfully!');
}

createIcons().catch(console.error);
