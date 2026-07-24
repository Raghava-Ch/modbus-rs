const { spawnSync } = require('child_process');
const fs = require('fs');
const path = require('path');

const baseDir = path.join(__dirname, '..');
const distNpmDir = path.join(baseDir, 'dist', 'npm');

function checkPackage(dir, name) {
  console.log(`\n=============================================================`);
  console.log(`📦 CHECKING PUBLISH CANDIDATE: ${name}`);
  console.log(`   Path: ${path.relative(baseDir, dir) || '.'}`);
  console.log(`=============================================================`);
  
  if (!fs.existsSync(dir)) {
    console.error(`❌ Directory not found. Please run 'npm run build' first.\n`);
    return;
  }
  
  // Use shell: true on Windows so 'npm' resolves correctly in spawnSync
  const result = spawnSync('npm', ['pack', '--dry-run'], { 
    cwd: dir, 
    stdio: 'inherit',
    shell: process.platform === 'win32'
  });
  
  if (result.status !== 0) {
    console.error(`❌ npm pack failed for ${name}\n`);
  }
}

console.log(`🔍 Starting npm publish candidate verification...`);

// 1. Check Main Package
checkPackage(baseDir, 'Main Package (modbus-rs)');

// 2. Check Sub-packages
if (fs.existsSync(distNpmDir)) {
  const subDirs = fs.readdirSync(distNpmDir, { withFileTypes: true })
    .filter(dirent => dirent.isDirectory())
    .map(dirent => dirent.name);
    
  if (subDirs.length === 0) {
    console.log(`\n⚠️ No sub-packages found in dist/npm/`);
  }

  for (const sub of subDirs) {
    checkPackage(path.join(distNpmDir, sub), `Sub-Package (modbus-rs-${sub})`);
  }
} else {
  console.log(`\n⚠️ The directory dist/npm/ does not exist. No sub-packages to check.`);
}

console.log(`\n✅ Finished checking all publish candidates. Review the output above to verify the files!\n`);
