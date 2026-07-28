// The extension entry point: wires up the language server, the indentation
// formatter, the Outline provider, and the `typl` CLI commands.
//
// Everything except the language server works with no external process at all --
// highlighting comes from the TextMate grammar, indentation from `indent.ts` and
// the Outline from `symbols.ts` -- so the extension stays useful in a checkout
// where `typl-lsp` has not been built.

import { existsSync } from "node:fs";
import { join } from "node:path";

import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from "vscode-languageclient/node";

// The indenter and the definition scanner are plain functions with no `vscode`
// dependency of their own, which is what lets `node --test` exercise them
// directly; this module is the only place they meet the editor API.
import { indentForLine, indentText } from "./indent";
import { Definition, DefinitionKind, findDefinitions } from "./symbols";

const LANGUAGE_ID = "typelisp";
const CONFIG_SECTION = "typelisp";

let client: LanguageClient | undefined;
// A log channel rather than a plain output channel: that is what
// `LanguageClientOptions.outputChannel` takes, and it gives the server's
// traffic the standard log-level filtering for free.
let output: vscode.LogOutputChannel;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  output = vscode.window.createOutputChannel("typelisp", { log: true });
  context.subscriptions.push(output);

  const selector: vscode.DocumentSelector = { language: LANGUAGE_ID, scheme: "file" };
  context.subscriptions.push(
    vscode.languages.registerDocumentFormattingEditProvider(selector, formattingProvider),
    vscode.languages.registerDocumentRangeFormattingEditProvider(selector, rangeFormattingProvider),
    vscode.languages.registerOnTypeFormattingEditProvider(selector, onTypeFormattingProvider, "\n", ")"),
    vscode.languages.registerDocumentSymbolProvider(selector, documentSymbolProvider),
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("typelisp.runFile", () => runCli([])),
    vscode.commands.registerCommand("typelisp.compileModule", () => runCli(["compile-module"])),
    vscode.commands.registerCommand("typelisp.repl", startRepl),
    vscode.commands.registerCommand("typelisp.restartServer", async () => {
      await stopClient();
      await startClient(context);
    }),
  );

  // A change to either server setting takes effect without a window reload.
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration(async (event) => {
      if (
        event.affectsConfiguration(`${CONFIG_SECTION}.languageServer.enable`) ||
        event.affectsConfiguration(`${CONFIG_SECTION}.languageServer.path`)
      ) {
        await stopClient();
        await startClient(context);
      }
    }),
  );

  await startClient(context);
}

export async function deactivate(): Promise<void> {
  await stopClient();
}

// ---------------------------------------------------------------- formatting

/** A single edit replacing a line's leading whitespace with `column` spaces. */
function indentEdit(
  document: vscode.TextDocument,
  line: number,
  column: number,
  existing: number,
): vscode.TextEdit | undefined {
  const replacement = " ".repeat(column);
  if (document.lineAt(line).text.slice(0, existing) === replacement && existing === column) {
    return undefined;
  }
  return vscode.TextEdit.replace(
    new vscode.Range(line, 0, line, existing),
    replacement,
  );
}

const formattingProvider: vscode.DocumentFormattingEditProvider = {
  provideDocumentFormattingEdits(document) {
    const original = document.getText();
    const formatted = indentText(original);
    if (formatted === original) {
      return [];
    }
    // One whole-document replacement: the indenter re-scans as it goes, so the
    // line-by-line edits are not independent of each other.
    const whole = new vscode.Range(
      document.positionAt(0),
      document.positionAt(original.length),
    );
    return [vscode.TextEdit.replace(whole, formatted)];
  },
};

const rangeFormattingProvider: vscode.DocumentRangeFormattingEditProvider = {
  provideDocumentRangeFormattingEdits(document, range) {
    const text = document.getText();
    const edits: vscode.TextEdit[] = [];
    // Computed against the unmodified document, so the edits stay independent
    // and VS Code can apply them as one batch. A selection is normally already
    // close to correct, which is what makes that safe here where the
    // whole-document path re-scans instead.
    for (let line = range.start.line; line <= range.end.line; line++) {
      if (document.lineAt(line).text.length === 0) {
        continue;
      }
      const want = indentForLine(text, line);
      if (!want) {
        continue;
      }
      const edit = indentEdit(document, line, want.column, want.existing);
      if (edit) {
        edits.push(edit);
      }
    }
    return edits;
  },
};

const onTypeFormattingProvider: vscode.OnTypeFormattingEditProvider = {
  provideOnTypeFormattingEdits(document, position) {
    // Both triggers land on the line to fix: after Enter, `position` is on the
    // freshly opened line; after `)`, it is just past the bracket, which is how
    // a closing paren finds its own column.
    const want = indentForLine(document.getText(), position.line);
    if (!want) {
      return [];
    }
    const edit = indentEdit(document, position.line, want.column, want.existing);
    return edit ? [edit] : [];
  },
};

// ---------------------------------------------------------------- outline

const SYMBOL_KINDS: Record<DefinitionKind, vscode.SymbolKind> = {
  function: vscode.SymbolKind.Function,
  method: vscode.SymbolKind.Method,
  macro: vscode.SymbolKind.Event,
  struct: vscode.SymbolKind.Struct,
  enum: vscode.SymbolKind.Enum,
  trait: vscode.SymbolKind.Interface,
  impl: vscode.SymbolKind.Namespace,
  variable: vscode.SymbolKind.Variable,
  constant: vscode.SymbolKind.Constant,
  module: vscode.SymbolKind.Module,
};

/** The detail string shown after a symbol's name in the Outline view. */
function detailOf(def: Definition): string {
  const kind = def.kind === "impl" ? "impl" : def.kind;
  return def.isPublic ? `pub ${kind}` : kind;
}

const documentSymbolProvider: vscode.DocumentSymbolProvider = {
  provideDocumentSymbols(document) {
    const text = document.getText();
    return findDefinitions(text).map((def) => {
      const range = new vscode.Range(
        document.positionAt(def.start),
        document.positionAt(def.end),
      );
      const selection = new vscode.Range(
        document.positionAt(def.nameStart),
        document.positionAt(def.nameEnd),
      );
      return new vscode.DocumentSymbol(
        def.name,
        detailOf(def),
        SYMBOL_KINDS[def.kind],
        range,
        selection,
      );
    });
  },
};

// ---------------------------------------------------------------- language server

/**
 * Where to find `typl-lsp`: the explicit setting if given, then the build
 * outputs of a typelisp checkout (release before debug, since a stale debug
 * binary next to a fresh release one is the less likely intent), then `PATH`.
 */
function resolveServerPath(): string {
  const configured = vscode.workspace
    .getConfiguration(CONFIG_SECTION)
    .get<string>("languageServer.path");
  if (configured) {
    return configured;
  }
  const exe = process.platform === "win32" ? "typl-lsp.exe" : "typl-lsp";
  for (const folder of vscode.workspace.workspaceFolders ?? []) {
    for (const profile of ["release", "debug"]) {
      const candidate = join(folder.uri.fsPath, "target", profile, exe);
      if (existsSync(candidate)) {
        return candidate;
      }
    }
  }
  // Left to `PATH`; if it is not there either, the client reports the failure
  // in its own output channel.
  return "typl-lsp";
}

async function startClient(context: vscode.ExtensionContext): Promise<void> {
  const config = vscode.workspace.getConfiguration(CONFIG_SECTION);
  if (!config.get<boolean>("languageServer.enable", true)) {
    output.appendLine("language server disabled by typelisp.languageServer.enable");
    return;
  }
  const command = resolveServerPath();
  output.appendLine(`starting language server: ${command}`);

  // Owned by the extension, not the client: `typelisp.restartServer` builds a
  // fresh client each time, and watchers left behind would accumulate.
  const watchers = [
    vscode.workspace.createFileSystemWatcher("**/*.typl"),
    vscode.workspace.createFileSystemWatcher("**/typelisp.toml"),
  ];
  context.subscriptions.push(...watchers);

  const serverOptions: ServerOptions = {
    run: { command, transport: TransportKind.stdio },
    debug: { command, transport: TransportKind.stdio },
  };
  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: LANGUAGE_ID }],
    outputChannel: output,
    synchronize: {
      // The server resolves `use` across files through the project's
      // typelisp.toml, so a change to one file can change another's
      // diagnostics; let it see both.
      fileEvents: watchers,
    },
  };

  client = new LanguageClient("typelisp", "typelisp language server", serverOptions, clientOptions);
  try {
    await client.start();
    context.subscriptions.push(client);
  } catch (err) {
    client = undefined;
    output.appendLine(`language server failed to start: ${err}`);
    void vscode.window.showWarningMessage(
      "typelisp: could not start typl-lsp. Highlighting, indentation and outline still work; " +
        "build it with `cargo build --release --bin typl-lsp` or set typelisp.languageServer.path.",
    );
  }
}

async function stopClient(): Promise<void> {
  const running = client;
  client = undefined;
  if (running) {
    await running.stop();
  }
}

// ---------------------------------------------------------------- commands

/** The `typl` executable, resolving a relative setting against the workspace. */
function resolveProgram(folder: vscode.WorkspaceFolder | undefined): string {
  const configured =
    vscode.workspace.getConfiguration(CONFIG_SECTION).get<string>("program") || "typl";
  const looksRelative = configured.startsWith("./") || configured.startsWith("../");
  return looksRelative && folder ? join(folder.uri.fsPath, configured) : configured;
}

/**
 * Run `typl [args...] FILE` on the active editor's file in a terminal.
 *
 * The file is saved first: the CLI reads from disk, so running an unsaved buffer
 * would silently check the previous revision.
 */
async function runCli(args: string[]): Promise<void> {
  const editor = vscode.window.activeTextEditor;
  if (!editor || editor.document.languageId !== LANGUAGE_ID) {
    void vscode.window.showErrorMessage("typelisp: no typelisp file is active");
    return;
  }
  if (editor.document.isUntitled) {
    void vscode.window.showErrorMessage("typelisp: save the file before running it");
    return;
  }
  await editor.document.save();

  const folder = vscode.workspace.getWorkspaceFolder(editor.document.uri);
  const terminal = terminalNamed("typelisp", folder);
  terminal.show(true);
  const parts = [resolveProgram(folder), ...args, editor.document.uri.fsPath];
  terminal.sendText(parts.map(quoteForShell).join(" "));
}

function startRepl(): void {
  const folder = vscode.workspace.workspaceFolders?.[0];
  const terminal = vscode.window.createTerminal({
    name: "typelisp REPL",
    cwd: folder?.uri.fsPath,
  });
  terminal.show(true);
  terminal.sendText(quoteForShell(resolveProgram(folder)));
}

/** Reuse the named terminal if it is still alive, so runs do not pile up tabs. */
function terminalNamed(name: string, folder: vscode.WorkspaceFolder | undefined): vscode.Terminal {
  const existing = vscode.window.terminals.find((t) => t.name === name && t.exitStatus === undefined);
  return existing ?? vscode.window.createTerminal({ name, cwd: folder?.uri.fsPath });
}

/**
 * Quote an argument for a POSIX shell, and for PowerShell on Windows -- a path
 * with a space in it is otherwise split into two arguments.
 */
function quoteForShell(arg: string): string {
  if (/^[A-Za-z0-9_/:\\.\-]+$/.test(arg)) {
    return arg;
  }
  if (process.platform === "win32") {
    return `'${arg.replace(/'/g, "''")}'`;
  }
  return `'${arg.replace(/'/g, "'\\''")}'`;
}
