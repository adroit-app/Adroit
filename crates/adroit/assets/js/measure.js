const s = document.createElement('span');
s.className = 'cell-probe';
s.textContent = 'M'.repeat(100);
document.body.appendChild(s);
const w = s.getBoundingClientRect().width / 100;
// A line as tall as the font asks for: its ascent, descent and gap, which is
// how a terminal sizes its cells.
s.style.lineHeight = 'normal';
s.style.display = 'inline-block';
const h = s.getBoundingClientRect().height;
s.remove();
dioxus.send([w, h]);
