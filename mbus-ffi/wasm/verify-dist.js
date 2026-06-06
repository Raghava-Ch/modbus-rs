import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const packageJsonPath = path.join(__dirname, 'package.json');
const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));

const files = packageJson.files;
if (!files || !Array.isArray(files)) {
  console.error('No "files" array found in package.json');
  process.exit(1);
}

let hasError = false;
for (const file of files) {
  const filePath = path.join(__dirname, file);
  if (!fs.existsSync(filePath)) {
    console.error(`❌ Missing file listed in package.json files array: ${file}`);
    hasError = true;
  } else {
    const stats = fs.statSync(filePath);
    if (stats.size === 0) {
      console.error(`❌ File listed in package.json files array is empty (0 bytes): ${file}`);
      hasError = true;
    } else {
      console.log(`✅ Verified file: ${file} (${stats.size} bytes)`);
    }
  }
}

if (hasError) {
  process.exit(1);
}
console.log('🎉 All files listed in package.json files array successfully verified!');
