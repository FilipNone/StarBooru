// @ts-check
// StarBooru Runner: one play button per thing you can start. Starting
// something also starts what it needs (web app → server → database), waits
// until it answers, and opens it.
'use strict';

const vscode = require('vscode');
const http = require('http');
const net = require('net');
const fs = require('fs');
const os = require('os');
const path = require('path');
const crypto = require('crypto');
const { execFile } = require('child_process');

const DB_CONTAINER = 'starbooru-dev-pg';
const DB_VOLUME = 'starbooru-dev-pgdata';
const DB_IMAGE = 'pgvector/pgvector:pg17';
const DB_PASSWORD = 'starbooru-dev';
// Dev only; the Docker stack gets random secrets in .env.
const DEV_SECRET = 'starbooru-dev-secret-not-for-production-000000';
// A first build compiles every dependency; give it time.
const READY_TIMEOUT_MS = 20 * 60 * 1000;
const COMPOSE_FILES = ['-f', 'docker-compose.yml', '-f', 'docker-compose.local.yml', '-f', 'docker-compose.build.yml'];

/**
 * @typedef {'stopped' | 'starting' | 'running'} State
 * @typedef {{ id: string, label: string, icon: string, summary: string, needs?: string, openable?: boolean }} Target
 */

/** @type {Target[]} */
const TARGETS = [
  { id: 'web', label: 'Web app', icon: 'globe', summary: 'Hot-reloading web app (dx serve), opens in your browser', needs: 'server', openable: true },
  { id: 'desktop', label: 'Desktop app', icon: 'device-desktop', summary: 'Native window (dx serve --desktop)', needs: 'server' },
  { id: 'android', label: 'Android app', icon: 'device-mobile', summary: 'Emulator or device (dx serve --android, needs the Android SDK/NDK)', needs: 'server' },
  { id: 'server', label: 'Server', icon: 'server', summary: 'API server (cargo run), data in ./data', needs: 'database' },
  { id: 'database', label: 'Database', icon: 'database', summary: `PostgreSQL in Docker (${DB_CONTAINER}), data kept in volume ${DB_VOLUME}` },
  { id: 'docker', label: 'Docker stack', icon: 'package', summary: 'The production image and compose file, built locally', openable: true },
];

const byId = (/** @type {string} */ id) => TARGETS.find((t) => t.id === id);

/**
 * The setting that holds each target's host port. The desktop and Android apps have none.
 * @type {Record<string, 'webPort' | 'serverPort' | 'databasePort' | 'dockerPort'>}
 */
const PORT_KEYS = { web: 'webPort', server: 'serverPort', database: 'databasePort', docker: 'dockerPort' };
const MIN_PORT = 1024;
const MAX_PORT = 65535;

/** True if `id` is `ancestor` or needs it, directly or through another target. */
function dependsOn(/** @type {string} */ id, /** @type {string} */ ancestor) {
  for (let t = byId(id); t; t = t.needs ? byId(t.needs) : undefined) {
    if (t.id === ancestor) return true;
  }
  return false;
}

/** How many targets `id` needs before it can start (database 0, server 1, web 2). */
function depth(/** @type {string} */ id) {
  let n = 0;
  for (let t = byId(id); t?.needs; t = byId(t.needs)) n++;
  return n;
}

function settings() {
  const c = vscode.workspace.getConfiguration('starbooru');
  return {
    dxPath: c.get('dxPath', ''),
    serverPort: c.get('serverPort', 8080),
    webPort: c.get('webPort', 8081),
    dockerPort: c.get('dockerPort', 8090),
    databasePort: c.get('databasePort', 55432),
  };
}

/** The Dioxus CLI: the setting, else PATH, else ~/.cargo/bin. */
function findDx() {
  const configured = settings().dxPath;
  if (configured) return configured;
  const exe = process.platform === 'win32' ? 'dx.exe' : 'dx';
  for (const dir of (process.env.PATH || '').split(path.delimiter)) {
    if (dir && fs.existsSync(path.join(dir, exe))) return path.join(dir, exe);
  }
  const cargo = path.join(os.homedir(), '.cargo', 'bin', exe);
  return fs.existsSync(cargo) ? cargo : exe;
}

/** Quotes a program path for the integrated terminal's shell. */
function shellProgram(/** @type {string} */ program) {
  if (!/\s/.test(program)) return program;
  return process.platform === 'win32' ? `& "${program}"` : `"${program}"`;
}

/**
 * Runs a command and resolves with its outcome; never rejects.
 * @param {string} cmd @param {string[]} args @param {import('child_process').ExecFileOptions} [opts]
 * @returns {Promise<{ ok: boolean, out: string }>}
 */
function run(cmd, args, opts = {}) {
  return new Promise((resolve) => {
    execFile(cmd, args, { timeout: 10 * 60 * 1000, ...opts }, (err, stdout, stderr) => {
      resolve({ ok: !err, out: `${stdout}${stderr}`.trim() });
    });
  });
}

/** True once `url` answers with a non-error status. */
function probe(/** @type {string} */ url) {
  return new Promise((resolve) => {
    const req = http.get(url, { timeout: 2000 }, (res) => {
      res.resume();
      resolve((res.statusCode || 500) < 400);
    });
    req.on('timeout', () => req.destroy());
    req.on('error', () => resolve(false));
  });
}

/** True if nothing listens on 127.0.0.1:`port`, which is where every target binds. */
function portFree(/** @type {number} */ port) {
  return new Promise((resolve) => {
    const server = net.createServer();
    server.once('error', () => resolve(false));
    server.listen({ port, host: '127.0.0.1', exclusive: true }, () => server.close(() => resolve(true)));
  });
}

const sleep = (/** @type {number} */ ms) => new Promise((r) => setTimeout(r, ms));

class Runner {
  /** @param {string} root */
  constructor(root) {
    this.root = root;
    /** @type {Map<string, vscode.Terminal>} */
    this.terminals = new Map();
    /** @type {Map<string, State>} */
    this.states = new Map();
    /** @type {Map<string, Promise<boolean>>} */
    this.pending = new Map();
    /** @type {vscode.EventEmitter<void>} */
    this.changed = new vscode.EventEmitter();

    // Pick up terminals that survived a window reload.
    for (const term of vscode.window.terminals) {
      const target = TARGETS.find((t) => term.name === this.terminalName(t));
      if (target) {
        this.terminals.set(target.id, term);
        this.states.set(target.id, 'running');
      }
    }
  }

  /** @param {Target} t */
  terminalName(t) {
    return `StarBooru: ${t.label}`;
  }

  /** @param {string} id @returns {State} */
  state(id) {
    return this.states.get(id) || 'stopped';
  }

  /** @param {string} id @param {State} state */
  setState(id, state) {
    this.states.set(id, state);
    this.changed.fire();
  }

  serverUrl() {
    return `http://127.0.0.1:${settings().serverPort}`;
  }

  /** Where a running target is reached, for the tree and the open button. */
  url(/** @type {string} */ id) {
    const s = settings();
    switch (id) {
      case 'web': return `http://127.0.0.1:${s.webPort}`;
      case 'server': return this.serverUrl();
      case 'docker': return `http://127.0.0.1:${s.dockerPort}`;
      case 'database': return `postgres://127.0.0.1:${s.databasePort}`;
      default: return undefined;
    }
  }

  /**
   * How a terminal target is launched and when it counts as ready.
   * @param {string} id
   * @returns {{ command: string, env: Record<string, string>, ready?: string } | undefined}
   */
  launch(id) {
    const s = settings();
    const dx = shellProgram(findDx());
    switch (id) {
      case 'server':
        return {
          command: 'cargo build -p starbooru-ai-worker && cargo run -p starbooru-server',
          env: {
            DATABASE_URL: `postgres://starbooru:${DB_PASSWORD}@127.0.0.1:${s.databasePort}/starbooru`,
            STARBOORU_SECRET: DEV_SECRET,
            STARBOORU_BIND: `127.0.0.1:${s.serverPort}`,
            STARBOORU_DATA_DIR: path.join(this.root, 'data'),
            STARBOORU_ALLOW_REGISTRATION: 'true',
            STARBOORU_NAME: 'StarBooru (dev)',
          },
          ready: `${this.serverUrl()}/healthz`,
        };
      case 'web':
        return {
          command: `${dx} serve --web -p starbooru-app --addr 127.0.0.1 --port ${s.webPort} --open false`,
          env: { STARBOORU_SERVER: this.serverUrl() },
          ready: this.url('web'),
        };
      case 'desktop':
        return {
          command: `${dx} serve --desktop -p starbooru-app`,
          env: { STARBOORU_SERVER: this.serverUrl() },
        };
      case 'android':
        return {
          // The emulator reaches the host's localhost at 10.0.2.2.
          command: `${dx} serve --android -p starbooru-app`,
          env: { STARBOORU_SERVER: `http://10.0.2.2:${s.serverPort}` },
        };
      case 'docker':
        return {
          command: `docker compose ${COMPOSE_FILES.join(' ')} up --build`,
          env: { STARBOORU_PORT: String(s.dockerPort) },
          ready: `${this.url('docker')}/healthz`,
        };
      default:
        return undefined;
    }
  }

  /**
   * Starts a target (and what it needs). Resolves true once it is ready.
   * @param {string} id @param {{ open?: boolean }} [opts]
   * @returns {Promise<boolean>}
   */
  async start(id, opts = { open: true }) {
    const target = byId(id);
    if (!target) return false;
    const state = this.state(id);
    if (state === 'running') {
      if (opts.open && target.openable) this.open(id);
      return true;
    }
    const pending = this.pending.get(id);
    if (pending) return pending;

    const job = this.startFresh(target, Boolean(opts.open)).finally(() => this.pending.delete(id));
    this.pending.set(id, job);
    return job;
  }

  /**
   * Starts a target after checking that the ports it and its needs bind are free.
   * A taken port offers Change Port…; the start goes on with the new one.
   * @param {string} id
   */
  async startChecked(id) {
    return (await this.ensurePortsFree(id)) && this.start(id);
  }

  /** @param {string} id */
  async ensurePortsFree(id) {
    for (let t = byId(id); t; t = t.needs ? byId(t.needs) : undefined) {
      const key = PORT_KEYS[t.id];
      // Compose reports its own port clash in the terminal; a stack from an earlier session would look like one here.
      if (!key || t.id === 'docker') continue;
      const running = t.id === 'database' ? await this.databaseRunning() : this.state(t.id) !== 'stopped';
      if (running) continue;
      while (!(await portFree(settings()[key]))) {
        const pick = await vscode.window.showErrorMessage(
          `StarBooru: port ${settings()[key]} for the ${t.label.toLowerCase()} is taken by another program.`,
          'Change Port…',
        );
        if (pick !== 'Change Port…' || (await this.changePort(t.id)) === undefined) return false;
      }
    }
    return true;
  }

  /**
   * Asks for a new port, saves it, and offers to restart what still uses the old one.
   * @param {string} id @returns {Promise<number | undefined>} the new port, or undefined if cancelled
   */
  async changePort(id) {
    const target = byId(id);
    const key = PORT_KEYS[id];
    if (!target || !key) return undefined;
    const s = settings();
    const current = s[key];
    const taken = Object.entries(PORT_KEYS).filter(([other]) => other !== id).map(([other, k]) => ({ label: byId(other)?.label, port: s[k] }));
    const text = await vscode.window.showInputBox({
      title: `StarBooru: port for the ${target.label.toLowerCase()}`,
      prompt: id === 'database'
        ? 'Host port of the dev PostgreSQL container. Its data stays in its volume when the port changes.'
        : `Host port the ${target.label.toLowerCase()} listens on (127.0.0.1).`,
      value: String(current),
      validateInput: async (input) => {
        const port = Number(input.trim());
        if (!/^\d+$/.test(input.trim()) || port < MIN_PORT || port > MAX_PORT) {
          return `Enter a whole number from ${MIN_PORT} to ${MAX_PORT}.`;
        }
        const clash = taken.find((t) => t.port === port);
        if (clash) return `The ${clash.label?.toLowerCase()} already uses port ${port}.`;
        if (port !== current && !(await portFree(port))) {
          return { message: `Port ${port} is in use right now.`, severity: vscode.InputBoxValidationSeverity.Warning };
        }
        return undefined;
      },
    });
    if (text === undefined) return undefined;
    const port = Number(text.trim());
    if (port === current) return port;

    // Write where the value comes from now, so a workspace value doesn't hide a user one.
    const config = vscode.workspace.getConfiguration('starbooru');
    const where = config.inspect(key);
    const scope = where?.workspaceFolderValue !== undefined
      ? vscode.ConfigurationTarget.WorkspaceFolder
      : where?.workspaceValue !== undefined
        ? vscode.ConfigurationTarget.Workspace
        : vscode.ConfigurationTarget.Global;
    await config.update(key, port, scope);

    // The target itself and anything running on top of it (the server's DATABASE_URL, the apps' server address).
    const affected = TARGETS.filter((t) => this.state(t.id) !== 'stopped' && dependsOn(t.id, id));
    if (affected.length > 0) {
      const names = affected.map((t) => t.label).join(', ');
      const pick = await vscode.window.showInformationMessage(
        `StarBooru: ${target.label} now uses port ${port}. Restart ${names} to switch?`,
        'Restart',
      );
      if (pick === 'Restart') {
        for (const t of [...affected].sort((a, b) => depth(b.id) - depth(a.id))) await this.stop(t.id);
        for (const t of [...affected].sort((a, b) => depth(a.id) - depth(b.id))) await this.start(t.id, { open: t.id === id });
      }
    }
    return port;
  }

  /** @param {Target} target @param {boolean} open */
  async startFresh(target, open) {
    const id = target.id;
    this.setState(id, 'starting');
    try {
      if (target.needs && !(await this.start(target.needs, { open: false }))) {
        this.setState(id, 'stopped');
        return false;
      }
      if (id === 'database') return await this.startDatabase();
      if (id === 'docker' && !(await this.ensureEnvFile())) {
        this.setState(id, 'stopped');
        return false;
      }

      const launch = this.launch(id);
      if (!launch) return false;
      const term = vscode.window.createTerminal({
        name: this.terminalName(target),
        cwd: this.root,
        env: launch.env,
        iconPath: new vscode.ThemeIcon(target.icon),
      });
      this.terminals.set(id, term);
      term.sendText(launch.command);
      term.show(true);

      if (launch.ready && !(await this.waitUntilReady(target, launch.ready))) {
        // Cancelled or timed out: leave the terminal open so the error stays readable.
        if (this.terminals.has(id)) this.setState(id, 'running');
        return false;
      }
      if (!this.terminals.has(id)) return false; // closed while starting
      this.setState(id, 'running');
      if (open && target.openable) this.open(id);
      return true;
    } catch (err) {
      this.setState(id, 'stopped');
      vscode.window.showErrorMessage(`StarBooru: could not start ${target.label}: ${err}`);
      return false;
    }
  }

  /** @param {Target} target @param {string} url */
  waitUntilReady(target, url) {
    return vscode.window.withProgress(
      {
        location: vscode.ProgressLocation.Notification,
        title: `Starting ${target.label}`,
        cancellable: true,
      },
      async (progress, token) => {
        progress.report({ message: 'building… (output in the terminal)' });
        const deadline = Date.now() + READY_TIMEOUT_MS;
        while (Date.now() < deadline) {
          if (token.isCancellationRequested || !this.terminals.has(target.id)) return false;
          if (await probe(url)) return true;
          await sleep(1500);
        }
        vscode.window.showWarningMessage(`StarBooru: ${target.label} did not answer at ${url}. Check its terminal.`);
        return false;
      },
    );
  }

  async databaseRunning() {
    const r = await run('docker', ['inspect', '-f', '{{.State.Running}}', DB_CONTAINER]);
    return r.ok && r.out === 'true';
  }

  async startDatabase() {
    const id = 'database';
    if (await this.databaseRunning()) {
      this.setState(id, 'running');
      return true;
    }
    return vscode.window.withProgress(
      { location: vscode.ProgressLocation.Notification, title: 'Starting the database' },
      async (progress) => {
        const port = settings().databasePort;
        let exists = (await run('docker', ['container', 'inspect', DB_CONTAINER])).ok;
        if (exists) {
          // A container's port is fixed when it is made; after a port change, make it again on the same volume.
          const bound = await run('docker', ['inspect', '-f', '{{range $p, $b := .HostConfig.PortBindings}}{{range $b}}{{.HostPort}}{{end}}{{end}}', DB_CONTAINER]);
          if (bound.ok && bound.out !== String(port)) {
            progress.report({ message: `moving to port ${port} (data stays in ${DB_VOLUME})…` });
            const removed = await run('docker', ['rm', DB_CONTAINER]);
            if (!removed.ok) {
              this.setState(id, 'stopped');
              vscode.window.showErrorMessage(`StarBooru: could not move the database to port ${port}.\n${removed.out}`);
              return false;
            }
            exists = false;
          }
        }
        progress.report({ message: exists ? 'starting container…' : `creating container (first time pulls ${DB_IMAGE})…` });
        const started = exists
          ? await run('docker', ['start', DB_CONTAINER])
          : await run('docker', [
              'run', '-d', '--name', DB_CONTAINER,
              '-p', `127.0.0.1:${port}:5432`,
              '-e', 'POSTGRES_USER=starbooru',
              '-e', `POSTGRES_PASSWORD=${DB_PASSWORD}`,
              '-e', 'POSTGRES_DB=starbooru',
              '-v', `${DB_VOLUME}:/var/lib/postgresql/data`,
              DB_IMAGE,
            ]);
        if (!started.ok) {
          this.setState(id, 'stopped');
          vscode.window.showErrorMessage(`StarBooru: the database did not start. Is Docker running?\n${started.out}`);
          return false;
        }
        progress.report({ message: 'waiting for PostgreSQL…' });
        for (let i = 0; i < 60; i++) {
          // Over TCP: during first-time init Postgres runs a socket-only temporary server.
          if ((await run('docker', ['exec', DB_CONTAINER, 'pg_isready', '-h', '127.0.0.1', '-U', 'starbooru', '-d', 'starbooru'])).ok) {
            this.setState(id, 'running');
            return true;
          }
          await sleep(1000);
        }
        this.setState(id, 'stopped');
        vscode.window.showErrorMessage(`StarBooru: PostgreSQL did not become ready. See \`docker logs ${DB_CONTAINER}\`.`);
        return false;
      },
    );
  }

  /** The Docker stack needs secrets; make a gitignored .env with random ones. */
  async ensureEnvFile() {
    const envPath = path.join(this.root, '.env');
    if (fs.existsSync(envPath)) return true;
    const examplePath = path.join(this.root, '.env.example');
    if (!fs.existsSync(examplePath)) {
      vscode.window.showErrorMessage('StarBooru: .env.example is missing, cannot create .env for Docker.');
      return false;
    }
    const filled = fs
      .readFileSync(examplePath, 'utf8')
      .replace(/^POSTGRES_PASSWORD=.*$/m, `POSTGRES_PASSWORD=${crypto.randomBytes(24).toString('hex')}`)
      .replace(/^STARBOORU_SECRET=.*$/m, `STARBOORU_SECRET=${crypto.randomBytes(32).toString('hex')}`);
    fs.writeFileSync(envPath, filled, { mode: 0o600 });
    vscode.window.showInformationMessage('StarBooru: created .env with random secrets for the Docker stack (it is gitignored).');
    return true;
  }

  /** @param {string} id */
  async stop(id) {
    const target = byId(id);
    if (!target) return;
    if (id === 'database') {
      await run('docker', ['stop', DB_CONTAINER]);
      this.setState(id, 'stopped');
      return;
    }
    if (id === 'docker') {
      await vscode.window.withProgress(
        { location: vscode.ProgressLocation.Notification, title: 'Stopping the Docker stack' },
        () => run('docker', ['compose', ...COMPOSE_FILES, 'stop'], {
          cwd: this.root,
          env: { ...process.env, STARBOORU_PORT: String(settings().dockerPort) },
        }),
      );
    }
    const term = this.terminals.get(id);
    this.terminals.delete(id);
    this.setState(id, 'stopped');
    term?.dispose();
  }

  async stopAll() {
    for (const t of TARGETS) {
      if (this.state(t.id) !== 'stopped') await this.stop(t.id);
    }
  }

  /** @param {string} id */
  async restart(id) {
    await this.stop(id);
    await this.startChecked(id);
  }

  /** @param {string} id */
  open(id) {
    const url = this.url(id);
    if (url && byId(id)?.openable) vscode.env.openExternal(vscode.Uri.parse(url));
  }

  /** @param {string} id */
  showTerminal(id) {
    this.terminals.get(id)?.show();
  }

  /** @param {vscode.Terminal} term */
  onTerminalClosed(term) {
    for (const [id, t] of this.terminals) {
      if (t === term) {
        this.terminals.delete(id);
        this.setState(id, 'stopped');
      }
    }
  }

  async refresh() {
    this.setState('database', (await this.databaseRunning()) ? 'running' : 'stopped');
  }
}

/** @implements {vscode.TreeDataProvider<Target>} */
class TargetTree {
  /** @param {Runner} runner */
  constructor(runner) {
    this.runner = runner;
    this.onDidChangeTreeData = runner.changed.event;
  }

  getChildren() {
    return TARGETS;
  }

  /** @param {Target} t */
  getTreeItem(t) {
    const state = this.runner.state(t.id);
    const item = new vscode.TreeItem(t.label, vscode.TreeItemCollapsibleState.None);
    item.id = t.id;
    const url = this.runner.url(t.id);
    const key = PORT_KEYS[t.id];
    item.description =
      state === 'starting' ? 'starting…' : state === 'running' ? (url || 'running') : key ? `port ${settings()[key]}` : '';
    item.tooltip = new vscode.MarkdownString(
      `**${t.label}** (${state})\n\n${t.summary}${t.needs ? `\n\nStarts ${byId(t.needs)?.label.toLowerCase()} first.` : ''}`,
    );
    item.iconPath =
      state === 'starting'
        ? new vscode.ThemeIcon('loading~spin')
        : new vscode.ThemeIcon(t.icon, state === 'running' ? new vscode.ThemeColor('testing.iconPassed') : undefined);
    item.contextValue = [state, t.openable && 'openable', t.id !== 'database' && 'terminal', key && 'port'].filter(Boolean).join(' ');
    if (state !== 'stopped' && t.id !== 'database') {
      item.command = { command: 'starbooru.showTerminal', title: 'Show Output', arguments: [t] };
    }
    return item;
  }
}

/** Commands get a tree element, an id, or nothing (palette: ask). */
async function pickTarget(/** @type {unknown} */ arg, /** @type {string} */ verb, onlyWithPort = false) {
  if (typeof arg === 'string') return byId(arg)?.id;
  if (arg && typeof arg === 'object' && 'id' in arg) return byId(String(arg.id))?.id;
  const s = settings();
  const choice = await vscode.window.showQuickPick(
    TARGETS.filter((t) => !onlyWithPort || PORT_KEYS[t.id]).map((t) => ({
      label: t.label,
      description: onlyWithPort ? `port ${s[PORT_KEYS[t.id]]}` : t.summary,
      id: t.id,
    })),
    { placeHolder: `${verb} which part of StarBooru?` },
  );
  return choice?.id;
}

/** @param {vscode.ExtensionContext} context */
function activate(context) {
  const folder = vscode.workspace.workspaceFolders?.find((f) =>
    fs.existsSync(path.join(f.uri.fsPath, 'apps', 'app', 'Dioxus.toml')),
  );
  if (!folder) return;

  const runner = new Runner(folder.uri.fsPath);
  const tree = new TargetTree(runner);
  /** @param {string} name @param {(id: string) => unknown} fn @param {string} verb @param {boolean} [onlyWithPort] */
  const targetCommand = (name, fn, verb, onlyWithPort = false) =>
    vscode.commands.registerCommand(name, async (arg) => {
      const id = await pickTarget(arg, verb, onlyWithPort);
      if (id) await fn(id);
    });

  context.subscriptions.push(
    runner.changed,
    vscode.window.registerTreeDataProvider('starbooru.targets', tree),
    vscode.window.onDidCloseTerminal((t) => runner.onTerminalClosed(t)),
    targetCommand('starbooru.start', (id) => runner.startChecked(id), 'Start'),
    targetCommand('starbooru.stop', (id) => runner.stop(id), 'Stop'),
    targetCommand('starbooru.restart', (id) => runner.restart(id), 'Restart'),
    targetCommand('starbooru.open', (id) => runner.open(id), 'Open'),
    targetCommand('starbooru.showTerminal', (id) => runner.showTerminal(id), 'Show'),
    targetCommand('starbooru.changePort', (id) => runner.changePort(id), 'Change the port of', true),
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration('starbooru')) runner.changed.fire();
    }),
    vscode.commands.registerCommand('starbooru.stopAll', () => runner.stopAll()),
    vscode.commands.registerCommand('starbooru.refresh', () => runner.refresh()),
  );
  runner.refresh();
}

function deactivate() {}

module.exports = { activate, deactivate };
