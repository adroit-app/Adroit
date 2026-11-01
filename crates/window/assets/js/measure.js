const s = document.createElement('span');
s.className = 'cell-probe';
s.textContent = 'M'.repeat(100);
document.body.appendChild(s);
// A font still loading measures as the one standing in for it, whose cells
// are narrower or wider than those the screen is drawn in once it loads.
const probed = getComputedStyle(s);
await document.fonts.load(`${probed.fontSize} ${probed.fontFamily}`, s.textContent);
const w = s.getBoundingClientRect().width / 100;
// A line as tall as the font asks for: its ascent, descent and gap, which is
// how a terminal sizes its cells.
s.style.lineHeight = 'normal';
s.style.display = 'inline-block';
const h = s.getBoundingClientRect().height;
s.remove();
dioxus.send([w, h]);
