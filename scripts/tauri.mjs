import { execFileSync, spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const env = { ...process.env };

// Windows 上把 Rust 工具链和 MSVC 编译环境传给 Tauri CLI。
if (process.platform === 'win32') {
  const cargoHome = join(root, '.toolchain', 'cargo');
  const rustupHome = join(root, '.toolchain', 'rustup');
  if (existsSync(cargoHome)) {
    env.CARGO_HOME = cargoHome;
    env.RUSTUP_HOME = rustupHome;
    env.PATH = `${join(cargoHome, 'bin')};${env.PATH}`;
  }

  const vswhere = 'C:\\Program Files (x86)\\Microsoft Visual Studio\\Installer\\vswhere.exe';
  if (existsSync(vswhere)) {
    try {
      const installPath = execFileSync(
        vswhere,
        ['-latest', '-products', '*', '-property', 'installationPath'],
        { encoding: 'utf8' }
      ).trim();
      const vcvars = join(installPath, 'VC', 'Auxiliary', 'Build', 'vcvars64.bat');
      if (installPath && existsSync(vcvars)) {
        const output = execFileSync(
          'cmd.exe',
          ['/d', '/s', '/c', `""${vcvars}" && set"`],
          {
            encoding: 'utf8',
            windowsHide: true,
            windowsVerbatimArguments: true,
          }
        );
        for (const line of output.split(/\r?\n/)) {
          const index = line.indexOf('=');
          if (index > 0) env[line.slice(0, index)] = line.slice(index + 1);
        }
      }
    } catch (error) {
      console.warn('未能加载 MSVC 编译环境，如果编译失败请安装 Visual Studio Build Tools。');
    }
  }
}

const cli = join(root, 'node_modules', '@tauri-apps', 'cli', 'tauri.js');
const args = process.argv.slice(2);
const child = existsSync(cli)
  ? spawn(process.execPath, [cli, ...args], { stdio: 'inherit', cwd: root, env })
  : spawn(process.platform === 'win32' ? 'npx.cmd' : 'npx', ['tauri', ...args], {
      stdio: 'inherit',
      cwd: root,
      env,
    });

child.on('exit', (code) => process.exit(code ?? 1));
