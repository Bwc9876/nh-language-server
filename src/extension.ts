import {
    window,
    workspace,
    ExtensionContext,
    Uri,
    ExtensionMode,
    commands,
    ViewColumn
} from "vscode";

import shipLogHtml from "./ship_logs.html?raw";

import {
    Executable,
    LanguageClient,
    LanguageClientOptions,
    ServerOptions
} from "vscode-languageclient/node";
import { ShipLogEntry } from "./types";

let client: LanguageClient;

const transportKind = {
    stdio: 0,
    ipc: 1,
    pipe: 2,
    socket: 3
};

async function activateServer(context: ExtensionContext) {
    const ext = process.platform === "win32" ? ".exe" : "";
    const mode = context.extensionMode === ExtensionMode.Development ? "debug" : "release";
    const bundled = Uri.joinPath(
        context.extensionUri,
        "server",
        "target",
        mode,
        `nh-language-server${ext}`
    );

    const serverExecutable: Executable = {
        command: bundled.fsPath,
        transport: transportKind.stdio
    };

    const serverOptions: ServerOptions = {
        run: serverExecutable,
        debug: {
            command: "cargo",
            args: ["run", "-q", "--"],
            transport: transportKind.stdio,
            options: {
                cwd: `${context.extensionPath}/server`
            }
        }
    };

    const clientOptions: LanguageClientOptions = {
        outputChannel: window.createOutputChannel("New Horizons Language Server"),
        documentSelector: [{ language: "xml" }, { language: "json" }],
        synchronize: {
            fileEvents: workspace.createFileSystemWatcher("**")
        }
    };

    client = new LanguageClient(
        "nh-lang-client",
        "New Horizons Language Server",
        serverOptions,
        clientOptions
    );

    client.start();
}

export async function activate(context: ExtensionContext) {
    context.subscriptions.push(
        commands.registerCommand("nh-language-server.restart", async () => {
            await client.stop();
            await activateServer(context).catch((e) => {
                void window.showErrorMessage(`Cannot start lsp: ${e.message}`);
                throw e;
            });
        }),
        commands.registerCommand("nh-language-server.ship-log-preview", async () => {
            if (!client) {
                return;
            }
            const systems: string[] = await client.sendRequest("getSystems");
            const chosenSystem = await window.showQuickPick(systems, { canPickMany: false });
            if (!chosenSystem) {
                return;
            }

            const entries: ShipLogEntry[] | null = await client.sendRequest(
                "getEntriesForSystem",
                chosenSystem
            );

            console.debug(entries);

            if (!entries) {
                window.showErrorMessage(`No entries found for ${chosenSystem}`);
                return;
            }

            const panel = window.createWebviewPanel(
                "shipLogPreview",
                `Ship Log Preview (${chosenSystem})`,
                ViewColumn.One,
                {}
            );

            const makeEntryLi = (e: ShipLogEntry) => `<li>${e.name}</li>`;
            const entriesHtml = `
                    <h1>Preview for ${chosenSystem}</h1>
                    <h2>Entries</h2>
                    <ul>${entries.map(makeEntryLi).join("")}</ul>
                `;

            panel.webview.html = shipLogHtml.replace("<!-- ~~ -->", entriesHtml);
        })
    );

    await activateServer(context).catch((e) => {
        void window.showErrorMessage(`Cannot activate nh-language-server extension: ${e.message}`);
        throw e;
    });
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
