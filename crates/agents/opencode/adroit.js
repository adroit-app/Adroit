// The plugin every OpenCode session adroit starts loads. adroit writes it
// afresh into its state folder before each session, so a change made to the
// copy there is lost.
//
// It runs in the server OpenCode starts for the one session's interface. It
// hands the session the messages other sessions send it, and gives the
// commands the session runs back the folders an account of its own moved
// OpenCode's away from.

import { spawn } from "node:child_process";

const ADROIT = "@ADROIT@";

/** The variable adroit names the conversation the interface opens in. */
const SESSION = "@SESSION@";

/** The prefix adroit keeps a variable's value from outside an account under. */
const OUTSIDE = "@OUTSIDE@";

/** How long to wait before listening again once the listener has ended. */
const AGAIN_MS = 5000;

/** How often a message waiting for a conversation not yet made is tried again. */
const RETRY_MS = 2000;

export default {
	id: "adroit",
	async setup(ctx) {
		const state = {
			// The conversation the interface shows: the one adroit chose, until
			// the user opens a new one.
			session: process.env[SESSION] || undefined,
			waiting: [],
			handing: false,
			listener: undefined,
			stopped: false,
		};
		await ctx.shell.hook("create.before", (event) => {
			for (const [name, value] of Object.entries(event.env)) {
				if (!name.startsWith(OUTSIDE)) continue;
				const outside = name.slice(OUTSIDE.length);
				if (value === "") delete event.env[outside];
				else event.env[outside] = value;
				delete event.env[name];
			}
		});
		const controller = new AbortController();
		(async () => {
			for await (const event of ctx.event.subscribe({ signal: controller.signal })) {
				if (event.type === "session.created" && event.data && !event.data.parentID) {
					state.session = event.data.sessionID ?? state.session;
				}
				if (event.type.startsWith("session.")) hand(ctx, state);
			}
		})().catch(() => {});
		const timer = setInterval(() => hand(ctx, state), RETRY_MS);
		// Only a session adroit keeps a record of carries the credential its
		// server acts with; any other OpenCode hears nothing.
		if (process.env.ADROIT_DELEGATION_TOKEN && process.env.ADROIT_DELEGATION_ROOT) {
			listen(ctx, state);
		}
		return () => {
			state.stopped = true;
			controller.abort();
			clearInterval(timer);
			state.listener?.kill();
		};
	},
};

/**
 * Hands the conversation the interface shows each message heard, as the
 * user's, queued behind the turn running. One the conversation cannot take
 * yet, as it is made only once its first prompt is sent, waits.
 */
async function hand(ctx, state) {
	if (state.handing || !state.session) return;
	state.handing = true;
	try {
		while (state.waiting.length > 0) {
			const { id, text } = state.waiting[0];
			// OpenCode takes a message whose id it holds as already taken, so
			// one tried again after a call that failed past taking it is not
			// handed twice.
			await ctx.session.prompt({ sessionID: state.session, id, text, delivery: "queue" });
			state.waiting.shift();
		}
	} catch {
	} finally {
		state.handing = false;
	}
}

/** The id of a message heard now, in the form OpenCode makes its own. */
function messageID() {
	const now = BigInt(Date.now()) * 0x1000n + BigInt(++heard % 0x1000);
	const time = now.toString(16).padStart(12, "0").slice(-12);
	const letters = Array.from(crypto.getRandomValues(new Uint8Array(14)), (byte) => LETTERS[byte % LETTERS.length]);
	return `msg_${time}${letters.join("")}`;
}

const LETTERS = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/** How many messages have been heard, which tells apart ids made in one millisecond. */
let heard = 0;

/**
 * Hears the messages other sessions send this one through adroit's server,
 * as Claude Code hears them through its channel. The server takes a message
 * as received once it has written it here.
 */
function listen(ctx, state) {
	const child = spawn(ADROIT, ["mcp", "--channel"], { stdio: ["pipe", "pipe", "ignore"] });
	state.listener = child;
	const send = (message) => child.stdin.write(`${JSON.stringify(message)}\n`);
	child.on("error", () => {});
	child.stdin.on("error", () => {});
	child.on("exit", () => {
		state.listener = undefined;
		if (!state.stopped) setTimeout(() => state.listener ?? listen(ctx, state), AGAIN_MS).unref();
	});
	send({
		jsonrpc: "2.0",
		id: 1,
		method: "initialize",
		params: {
			protocolVersion: "2025-06-18",
			capabilities: {},
			clientInfo: { name: "adroit-opencode", version: "1" },
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
				state.waiting.push({ id: messageID(), text: message.params.content });
				hand(ctx, state);
			}
		}
	});
}
