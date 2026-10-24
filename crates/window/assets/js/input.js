const t = document.getElementById('adroit-keys');
// This run of the script. The window runs it again once the keys are lost,
// and this run then leaves the page to the later one.
const me = {};
window.adroitInput = me;
const current = () => window.adroitInput === me;
// While keys are traced, what reaches the window and where focus goes.
if (window.adroitDebugKeys) {
  const trace = (text) => current() && dioxus.send({ kind: 'trace', text });
  const name = (el) => !el ? 'none' : el.tagName.toLowerCase() + (el.id ? '#' + el.id : '') + (typeof el.className === 'string' && el.className ? '.' + el.className.trim().split(/\s+/).join('.') : '');
  window.addEventListener('keydown', (e) => trace('keydown key=' + e.key + ' code=' + e.code + ' keyCode=' + e.keyCode + ' composing=' + e.isComposing + ' meta=' + e.metaKey + ' target=' + name(e.target) + ' active=' + name(document.activeElement) + ' readOnly=' + t.readOnly + ' connected=' + t.isConnected + ' focus=' + document.hasFocus()), true);
  document.addEventListener('focusin', (e) => trace('focusin ' + name(e.target)), true);
  document.addEventListener('focusout', (e) => trace('focusout ' + name(e.target)), true);
  window.addEventListener('focus', () => trace('window focus'));
  window.addEventListener('blur', () => trace('window blur'));
  for (const k of ['compositionstart', 'compositionend']) t.addEventListener(k, (e) => trace(k + ' data=' + e.data));
  window.addEventListener('error', (e) => trace('error ' + e.message + ' at ' + e.lineno + ':' + e.colno));
  window.addEventListener('unhandledrejection', (e) => trace('rejection ' + e.reason));
}
// Whether an input method composes in the text area. WebKit ends a
// composition whose text area turns read-only, is emptied or loses focus, and
// brings its text back later: none of these is done to it until the
// composition ends.
let composing = false;
const compose = (on) => {
  composing = on;
  if (current()) dioxus.send({ kind: 'composing', on });
};
// The window says in `data-takes-text` whether the text area takes text; it
// is made read-only, or writable, once no composition is open.
const settle = () => {
  if (composing || !current()) return;
  const takes = t.dataset.takesText !== 'no';
  if (t.readOnly === takes) t.readOnly = !takes;
};
new MutationObserver(settle).observe(t, { attributes: true, attributeFilter: ['data-takes-text'] });
settle();
// Whether the menu holds Command with this key: one of macOS's letters,
// whatever else is held, or a key an entry shows, written as the bindings
// write it. Shift is in a letter's case and in a character such as `}`, but
// not in a digit, which is read from its key.
const editsWithCommand = ['Backspace', 'Delete', 'ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'];
const held = (e, name, fromCode) => {
  const keys = window.adroitHeld || [];
  if (keys.includes(name.toLowerCase())) return true;
  const character = [...name].length === 1 && name !== ' ';
  const shift = e.shiftKey && !/^[A-Z]$/.test(name) && !(character && !fromCode);
  const shown = name === ' ' ? 'Space' : name === '+' ? 'Plus' : name;
  return keys.includes('Cmd+' + (e.ctrlKey ? 'Ctrl+' : '') + (e.altKey ? 'Alt+' : '') + (shift ? 'Shift+' : '') + shown);
};
// The Option key held, `AltLeft` or `AltRight`, and whether the settings
// count it as Alt: `window.adroitOptionAsAlt` names which do.
let option = '';
t.addEventListener('keyup', (e) => {
  if (e.code === option) option = '';
});
const optionIsAlt = () => {
  const as = window.adroitOptionAsAlt || 'both';
  return as === 'both' || (as === 'left' && option === 'AltLeft') || (as === 'right' && option === 'AltRight');
};
// Whether the user types in this field: a text field taking text, or a list
// to choose from, shown. A field hidden, gone or read-only would hold the keys
// without using them; taking focus from a list closes it as it opens.
const typesIn = (a) => (a instanceof HTMLInputElement || a instanceof HTMLTextAreaElement || a instanceof HTMLSelectElement)
  && a !== t && a.isConnected && !a.readOnly && !a.disabled && a.getClientRects().length > 0;
const take = () => {
  if (!current()) return;
  const a = document.activeElement;
  if (a !== t && !typesIn(a)) t.focus();
};
const keep = () => setTimeout(take, 0);
document.addEventListener('mouseup', keep);
window.addEventListener('focus', keep);
// Rust gives the keys back to the page when the window has them and the page
// has not.
const told = () => current() && dioxus.send({ kind: 'focused', on: document.hasFocus() });
window.addEventListener('focus', told);
window.addEventListener('blur', told);
told();
t.addEventListener('blur', keep);
// Whether the keys go to a terminal grid, where a wide character takes two
// cells rather than the width its font draws it in.
let onGrid = false;
let gridCursor = null;
// A key pressed on the text area, or `astray` on whatever else focus was on.
const onKey = (e, astray) => {
  if (!current()) return;
  // WebKit sends the key that ends a composition after `compositionend`,
  // no longer composing but still with key code 229.
  // Command with a key the menu does not hold is the window's own; the
  // letter is read from the key itself, as Command and Option change what it
  // types.
  if (e.metaKey && !e.isComposing && !['Meta', 'Shift', 'Control', 'Alt'].includes(e.key)) {
    const code = /^(Key[A-Z]|Digit[0-9])$/.exec(e.code);
    let name = code ? e.code.slice(-1) : e.key;
    if (code && !e.shiftKey) name = name.toLowerCase();
    // While the text area takes text, WebKit spends Command with a key that
    // edits, as Command-Backspace deleting to the line's start, on the text
    // area before the menu sees it: the window runs the binding instead.
    if (held(e, name, !!code) && (t.readOnly || !editsWithCommand.includes(name))) return;
    e.preventDefault();
    dioxus.send({ kind: 'key', key: name, ctrl: e.ctrlKey, alt: e.altKey, shift: e.shiftKey, meta: true });
    return;
  }
  if (e.isComposing || e.keyCode === 229 || e.metaKey) return;
  if (e.code === 'AltLeft' || e.code === 'AltRight') option = e.code;
  const printable = [...e.key].length === 1;
  // On a grid, an Option key that does not count as Alt types what macOS
  // makes of the key, as a terminal set so would.
  const alt = e.altKey && (!onGrid || optionIsAlt());
  if (printable && !e.ctrlKey && !alt) {
    // Read-only, the text area takes no text: the key is a command. Astray,
    // it never reached the text area to be typed.
    if (t.readOnly || astray) {
      e.preventDefault();
      dioxus.send({ kind: 'text', text: e.key });
    }
    return;
  }
  e.preventDefault();
  // Option turns a letter into another character on macOS (Option-h is
  // "˙"); the key itself is what a binding or a terminal's Meta means.
  let name = e.key;
  const base = e.altKey && /^(Key[A-Z]|Digit[0-9])$/.exec(e.code);
  if (base) {
    name = e.code.slice(-1);
    if (!e.shiftKey) name = name.toLowerCase();
  }
  dioxus.send({ kind: 'key', key: name, ctrl: e.ctrlKey, alt: e.altKey, shift: e.shiftKey });
};
t.addEventListener('keydown', (e) => onKey(e, false));
// A key that reaches the page while focus is where keys are not used, as on
// an element a click left focused, is the window's all the same. A dialog's
// keys are its own.
window.addEventListener('keydown', (e) => {
  const on = e.target;
  if (!current() || e.defaultPrevented || on === t || typesIn(on) || (on instanceof Element && on.closest('.dialog'))) return;
  t.focus();
  onKey(e, true);
});
// The text area sits on the cursor of what has the keys, so the input
// method draws its composition and opens its candidates there.
let at = '';
// The colour of the grid's cell under its cursor, which the text composed
// there is drawn in once it lands.
const colourAt = (cursor) => {
  const r = cursor.getBoundingClientRect();
  const x = r.left + 1, y = r.top + r.height / 2;
  for (const s of cursor.parentElement.querySelectorAll('.screen-line span')) {
    const b = s.getBoundingClientRect();
    if (x >= b.left && x < b.right && y >= b.top && y < b.bottom) return getComputedStyle(s).color;
  }
  return '';
};
const shown = document.getElementById('adroit-compose') || document.createElement('div');
shown.id = 'adroit-compose';
document.body.appendChild(shown);
const follow = () => {
  if (!current()) return;
  // The text area gone, as when the page is drawn anew, takes the listeners
  // with it: the window runs the script again.
  if (!t.isConnected) {
    dioxus.send({ kind: 'lost' });
    return;
  }
  // Focus left where the keys are not used, as on a text field gone or
  // hidden, gives them back to the text area.
  if (document.hasFocus()) take();
  const cursor = document.querySelector('.screen-cursor');
  const c = cursor
    || document.querySelector('.app-panel.focused .list-row.selected')
    || document.querySelector('.app-panel.focused');
  onGrid = !!cursor;
  gridCursor = cursor;
  if (c) {
    const r = c.getBoundingClientRect();
    const k = r.left + ',' + r.top;
    if (k !== at) {
      at = k;
      t.style.left = r.left + 'px';
      t.style.top = r.top + 'px';
      shown.style.left = r.left + 'px';
      shown.style.top = r.top + 'px';
    }
  }
  requestAnimationFrame(follow);
};
follow();
const probe = document.createElement('span');
probe.className = 'cell-probe';
document.body.appendChild(probe);
// East Asian wide and fullwidth characters, which a terminal gives two cells.
const wide = /[ᄀ-ᅟ⺀-〾ぁ-㏿㐀-䶿一-鿿ꀀ-꓏가-힣豈-﫿︰-﹏＀-｠￠-￦]|[\u{1F300}-\u{1F64F}\u{1F900}-\u{1F9FF}\u{20000}-\u{3FFFD}]/u;
let cell = 0;
t.addEventListener('compositionstart', () => {
  compose(true);
  t.classList.add('composing');
  if (onGrid) {
    probe.textContent = 'M'.repeat(100);
    cell = probe.getBoundingClientRect().width / 100;
    shown.style.color = gridCursor ? colourAt(gridCursor) : '';
    t.classList.add('gridded');
    shown.classList.add('on');
  }
});
t.addEventListener('compositionupdate', (e) => {
  const text = e.data || '';
  probe.textContent = text;
  t.style.width = Math.ceil(probe.getBoundingClientRect().width) + 2 + 'px';
  if (!onGrid) return;
  // Each character is drawn in the cells the grid will give it once it is
  // committed, and as large as the grid draws it, so the text neither spreads
  // out nor grows when it lands.
  const height = parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--cell-h'));
  shown.replaceChildren(...[...text].map((ch) => {
    const s = document.createElement('span');
    s.textContent = ch;
    const w = wide.test(ch);
    s.style.width = (w ? 2 : 1) * cell + 'px';
    if (w) s.style.fontSize = Math.min(2 * cell, height) + 'px';
    return s;
  }));
});
t.addEventListener('beforeinput', (e) => {
  if (e.isComposing || e.inputType === 'insertCompositionText') return;
  e.preventDefault();
  if (e.inputType === 'insertText' && e.data) dioxus.send({ kind: 'text', text: e.data });
});
t.addEventListener('paste', (e) => {
  e.preventDefault();
  const c = e.clipboardData;
  const d = c ? c.getData('text/plain') : '';
  if (d) {
    dioxus.send({ kind: 'paste', text: d });
    return;
  }
  // Only an image is read: Claude Code takes no video or other file.
  const f = c && [...c.items]
    .filter((i) => i.kind === 'file' && i.type.startsWith('image/'))
    .map((i) => i.getAsFile())
    .find((f) => f);
  if (!f) return;
  const r = new FileReader();
  r.onload = () => {
    const url = String(r.result);
    dioxus.send({ kind: 'image', type: f.type, data: url.slice(url.indexOf(',') + 1) });
  };
  r.readAsDataURL(f);
});
t.addEventListener('compositionend', (e) => {
  t.classList.remove('composing', 'gridded');
  shown.classList.remove('on');
  shown.replaceChildren();
  t.style.width = '';
  compose(false);
  if (e.data) dioxus.send({ kind: 'text', text: e.data });
  // A composition started meanwhile, as the key that commits one may start
  // the next, leaves the text area to its own end.
  setTimeout(() => {
    if (composing) return;
    t.value = '';
    settle();
  }, 0);
});
keep();
