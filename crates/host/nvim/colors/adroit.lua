-- Neovim's default highlights drawn in the colours of the theme Adroit's
-- console is in, read from the file $ADROIT_THEME_FILE names, and drawn again
-- whenever the work host rewrites it.

-- Kept across loads, as a colorscheme is sourced afresh each time it is chosen.
_G.adroit_colorscheme = _G.adroit_colorscheme or {}
local state = _G.adroit_colorscheme

local file = vim.env.ADROIT_THEME_FILE

---@return { ansi: integer[], background: integer, foreground: integer, cursor: integer }?
local function read(text)
  local theme = { ansi = {} }
  for line in vim.gsplit(text, '\n', { plain = true }) do
    local key, value = line:match('^%s*([%w-]+)%s*=%s*(.-)%s*$')
    local hex = function(v)
      local rgb = v and v:match('^#?(%x%x%x%x%x%x)$')
      return rgb and tonumber(rgb, 16)
    end
    if key == 'palette' then
      local index, colour = value:match('^(%d+)%s*=%s*(.-)$')
      if index then
        theme.ansi[tonumber(index)] = hex(colour)
      end
    elseif key == 'background' or key == 'foreground' then
      theme[key] = hex(value)
    elseif key == 'cursor-color' then
      theme.cursor = hex(value)
    end
  end
  for index = 0, 15 do
    if not theme.ansi[index] then
      return nil
    end
  end
  if not (theme.background and theme.foreground) then
    return nil
  end
  theme.cursor = theme.cursor or theme.foreground
  return theme
end

local function channels(rgb)
  return math.floor(rgb / 0x10000), math.floor(rgb / 0x100) % 0x100, rgb % 0x100
end

-- The same rule the console tells a light theme from a dark one by.
local function is_dark(theme)
  local function luminance(rgb)
    local r, g, b = channels(rgb)
    return 0.2126 * r + 0.7152 * g + 0.0722 * b
  end
  return luminance(theme.background) < luminance(theme.foreground)
end

-- `from` moved `by` of the way to `to`; past either end for `by` out of 0..1.
local function mix(from, to, by)
  local fr, fg, fb = channels(from)
  local tr, tg, tb = channels(to)
  local function at(f, t)
    return math.min(255, math.max(0, math.floor(f + (t - f) * by + 0.5)))
  end
  return at(fr, tr) * 0x10000 + at(fg, tg) * 0x100 + at(fb, tb)
end

-- How far each of Neovim's greys lies from the background towards the
-- foreground, on the background's side and on the text's.
local GREYS = {
  ground = { [1] = -0.07, [2] = 0, [3] = 0.12, [4] = 0.3 },
  text = { [1] = 1.07, [2] = 1, [3] = 0.86, [4] = 0.66 },
}

local HUES = { Red = 1, Green = 2, Yellow = 3, Blue = 4, Magenta = 5, Cyan = 6 }

-- How much of a hue is laid on the background where the default fills with it.
local GROUND_HUE = 0.4

-- Each colour of Neovim's own palette, by its value, as the theme draws it.
-- The default draws text in the colours of one half, Light or Dark, and
-- grounds in the other's; in a terminal's colours its text takes the bright
-- hues on a dark theme and the plain ones on a light, as here.
local function translation(theme, dark)
  local out = {}
  local text_half = dark and 'Light' or 'Dark'
  for name, value in pairs(vim.api.nvim_get_color_map()) do
    local half, what, step = name:match('^Nvim(Light)(%a-)(%d?)$')
    if not half then
      half, what, step = name:match('^Nvim(Dark)(%a-)(%d?)$')
    end
    if half then
      local side = half == text_half and 'text' or 'ground'
      if what == 'Grey' or what == 'Gray' then
        local by = GREYS[side][tonumber(step)]
        if by then
          out[value] = mix(theme.background, theme.foreground, by)
        end
      elseif HUES[what] and step == '' then
        local hue = HUES[what]
        if side == 'text' then
          out[value] = theme.ansi[dark and hue + 8 or hue]
        else
          out[value] = mix(theme.background, theme.ansi[hue], GROUND_HUE)
        end
      end
    end
  end
  return out
end

local function clear()
  vim.cmd('hi clear')
  if vim.g.syntax_on then
    vim.cmd('syntax reset')
  end
end

local function apply(theme)
  local dark = is_dark(theme)
  -- Cleared first, which forgets the colorscheme's name: with the name kept,
  -- a change of 'background' would load the colorscheme again from within.
  clear()
  vim.o.background = dark and 'dark' or 'light'
  clear()
  local colour = translation(theme, dark)
  for name, hl in pairs(vim.api.nvim_get_hl(0, {})) do
    if not hl.link then
      local changed = false
      for _, key in ipairs({ 'fg', 'bg', 'sp' }) do
        if hl[key] and colour[hl[key]] then
          hl[key] = colour[hl[key]]
          changed = true
        end
      end
      if changed then
        vim.api.nvim_set_hl(0, name, hl)
      end
    end
  end
  for index = 0, 15 do
    vim.g['terminal_color_' .. index] = string.format('#%06x', theme.ansi[index])
  end
end

local function stop()
  for _, key in ipairs({ 'watcher', 'retry' }) do
    if state[key] then
      state[key]:stop()
      state[key]:close()
      state[key] = nil
    end
  end
end

local function current()
  return file and vim.fn.filereadable(file) == 1 and table.concat(vim.fn.readfile(file), '\n')
end

local function changed()
  if vim.g.colors_name ~= 'adroit' then
    stop()
    return
  end
  local text = current()
  if text and text ~= state.text then
    vim.cmd.colorscheme('adroit')
  end
end

-- The work host replaces the file rather than writing into it, so it is the
-- folder that is watched: a watch on the file would end with the file it was
-- put on.
local function watch()
  if state.watcher or state.retry or not file then
    return
  end
  local watcher = vim.uv.new_fs_event()
  if not watcher then
    return
  end
  local ok = watcher:start(vim.fs.dirname(file), {}, function()
    vim.schedule(changed)
  end)
  if ok then
    state.watcher = watcher
    return
  end
  watcher:close()
  -- The folder is made with the work host's first write, which a Neovim
  -- started at once may come before: tried again until it can be watched.
  local retry = vim.uv.new_timer()
  if not retry then
    return
  end
  state.retry = retry
  retry:start(1000, 1000, function()
    vim.schedule(function()
      if state.retry ~= retry then
        return
      end
      if vim.fn.isdirectory(vim.fs.dirname(file)) == 1 then
        retry:stop()
        retry:close()
        state.retry = nil
        watch()
        changed()
      elseif vim.g.colors_name ~= 'adroit' then
        stop()
      end
    end)
  end)
end

local text = current()
local theme = text and read(text)
state.text = theme and text or nil
if theme then
  apply(theme)
else
  clear()
end
vim.g.colors_name = 'adroit'
watch()

vim.api.nvim_create_autocmd('ColorScheme', {
  group = vim.api.nvim_create_augroup('adroit_colorscheme', { clear = true }),
  callback = function(event)
    if event.match ~= 'adroit' then
      stop()
    end
  end,
})
