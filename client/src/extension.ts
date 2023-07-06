import { window, workspace, ExtensionContext } from 'vscode';

import {
    Executable,
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind,
} from 'vscode-languageclient/node';

let client: LanguageClient;

async function activateServer() {
    const serverExecutable: Executable = {
        "command": "./nh-language-server",
        "transport": TransportKind.stdio,
    };

    const serverOptions: ServerOptions = {
        run: serverExecutable,
        debug: {
            "command": "cargo",
            "args": ["run", "-q", "--"],
            "transport": TransportKind.stdio,
            "options": {
                "cwd": "/home/bean/Documents/GitHub/nh-language-server/server"
            }
        }
    };

    const clientOptions: LanguageClientOptions = {
        outputChannel: window.createOutputChannel("New Horizons Language Server"),
        documentSelector: [{ language: 'xml' }, { language: 'json' }],
        synchronize: {
            fileEvents: workspace.createFileSystemWatcher("**"),
        }
    };

    client = new LanguageClient(
        'nh-lang-client',
        'New Horizons Language Server',
        serverOptions,
        clientOptions
    );

    client.start();
}

export async function activate(context: ExtensionContext) {
    await activateServer().catch((e) => {
        void window.showErrorMessage(
            `Cannot activate nh-language-server extension: ${e.message}`
        );
        throw e;
    });
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}