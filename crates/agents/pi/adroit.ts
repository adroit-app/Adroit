// The extension every Pi session adroit starts loads. adroit writes it
// afresh into its state folder before each session, so a change made to
// the copy there is lost.
//
// It gives the session adroit's tools, hands it the messages other sessions
// send it, and keeps a record of the conversation the process holds, which
// adroit reads to follow a session to the conversation `/new` or `/resume`
// moves it to.

import { spawn } from "node:child_process";
import { mkdirSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const ADROIT = "@ADROIT@";
const RUNNING = "@RUNNING@";

/** How long to wait before listening again once the listener has ended. */
const AGAIN_MS = 5000;

// Pi loads the extension again for each conversation the process moves to,
// and the API it hands an earlier load refuses every call; what outlives a
// load is kept on the process: the one listener, the API of the conversation
// now open, the messages heard while none is, and whether the record is
// already removed when the process exits.
const shared = (globalThis[Symbol.for("adroit.pi")] ??= {
	pi: undefined,
	listener: undefined,
	waiting: [],
	forgetsOnExit: false,
});

/** When this process started, in seconds since the epoch, as adroit checks it. */
const STARTED = Math.round(Date.now() / 1000 - process.uptime());

export default function (pi) {
	// Only a session adroit keeps a record of carries the credential its
	// tools act with; any other Pi gets none of them.
	const delegated = Boolean(process.env.ADROIT_DELEGATION_TOKEN && process.env.ADROIT_DELEGATION_ROOT);
	if (delegated) {
		pi.registerMcpServer("adroit", {
			command: ADROIT,
			args: ["mcp"],
			exposure: "direct",
			description: "Adroit's tools for starting and messaging the sessions the user watches in Adroit",
		});
	}
	const record = join(RUNNING, `${process.pid}.json`);
	pi.on("session_start", async (_event, ctx) => {
		keep(record, { pid: process.pid, started: STARTED, sessionId: ctx.sessionManager.getSessionId() });
		shared.pi = pi;
		if (delegated && !shared.listener) {
			listen();
		}
		hand();
	});
	pi.on("session_shutdown", async () => {
		forget(record);
		if (shared.pi === pi) {
			shared.pi = undefined;
		}
	});
	if (!shared.forgetsOnExit) {
		shared.forgetsOnExit = true;
		process.once("exit", () => forget(record));
	}
}

function keep(path, value) {
	try {
		mkdirSync(RUNNING, { recursive: true, mode: 0o700 });
		const written = `${path}.${process.pid}.tmp`;
		writeFileSync(written, JSON.stringify(value));
		renameSync(written, path);
	} catch {}
}

function forget(path) {
	try {
		rmSync(path, { force: true });
	} catch {}
}

/**
 * Hands Pi each message heard, as the user's: it starts a turn while Pi
 * waits, and follows the turn running. One heard while no conversation is
 * open waits for the next.
 */
function hand() {
	while (shared.pi && shared.waiting.length > 0) {
		try {
			shared.pi.sendUserMessage(shared.waiting[0], { deliverAs: "followUp" });
			shared.waiting.shift();
		} catch {
			shared.pi = undefined;
		}
	}
}

/**
 * Hears the messages other sessions send this one through adroit's server,
 * as Claude Code hears them through its channel. The server takes a message
 * as received once it has written it here.
 */
function listen() {
	const child = spawn(ADROIT, ["mcp", "--channel"], { stdio: ["pipe", "pipe", "ignore"] });
	shared.listener = child;
	const send = (message) => child.stdin.write(`${JSON.stringify(message)}\n`);
	child.on("error", () => {});
	child.stdin.on("error", () => {});
	child.on("exit", () => {
		shared.listener = undefined;
		setTimeout(() => shared.listener ?? listen(), AGAIN_MS).unref();
	});
	send({
		jsonrpc: "2.0",
		id: 1,
		method: "initialize",
		params: {
			protocolVersion: "2025-06-18",
			capabilities: {},
			clientInfo: { name: "adroit-pi", version: "1" },
		},
	});
	let buffered = "";
	child.stdout.setEncoding("utf8");
	child.stdout.on("data", (chunk) => {
		buffered += chunk;
		for (let at = buffered.indexOf("\n"); at >= 0; at = buffered.indexOf("\n")) {
			const line = buffered.slice(0, at);
			buffered = buffered.slice(at + 1);
			let message;
			try {
				message = JSON.parse(line);
			} catch {
				continue;
			}
			if (message.id === 1) {
				send({ jsonrpc: "2.0", method: "notifications/initialized" });
			} else if (
				message.method === "notifications/claude/channel" &&
				typeof message.params?.content === "string"
			) {
				shared.waiting.push(message.params.content);
				hand();
			}
		}
	});
}
