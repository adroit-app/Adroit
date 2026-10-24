// The extension a Pi adroit opens to log in loads: Pi logs in only from its
// own interface, so it is opened with `/login` typed in, waiting for Enter.

export default function (pi) {
	pi.on("session_start", async (_event, ctx) => {
		ctx.ui.setEditorText("/login");
		ctx.ui.notify("Press Enter to log in to a provider; quit with ctrl+d once you are done.", "info");
	});
}
