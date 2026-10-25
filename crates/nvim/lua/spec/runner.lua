-- Runs one spec file in the Neovim it is started by:
--
--   nvim --clean --headless --cmd "lua <adroit's modules>" -l runner.lua x_spec.lua
--
-- A spec names what holds with `describe` and `it`, and checks it with `eq`
-- and `wait`. The Neovim ends failing when any `it` fails.
local passed, failures = 0, {}
local names = {}

function _G.describe(name, body)
  table.insert(names, name)
  body()
  table.remove(names)
end

function _G.it(name, body)
  local full = table.concat(names, " ") .. " " .. name
  local ok, err = xpcall(body, debug.traceback)
  if ok then
    passed = passed + 1
  else
    failures[#failures + 1] = full .. "\n" .. tostring(err)
  end
end

--- Fail unless `got` is `want`, deeply.
function _G.eq(want, got, what)
  if not vim.deep_equal(want, got) then
    error(("%swant %s, got %s"):format(what and what .. ": " or "", vim.inspect(want), vim.inspect(got)), 2)
  end
end

--- Let Neovim run what it has scheduled until `done` holds, failing after a
--- while.
function _G.wait(done, what)
  if not vim.wait(2000, done, 10) then
    error("never " .. (what or "done"), 2)
  end
end

dofile(_G.arg[1])
for _, f in ipairs(failures) do
  io.stderr:write("FAIL " .. f .. "\n\n")
end
io.stdout:write(("%d passed, %d failed\n"):format(passed, #failures))
os.exit(#failures == 0 and 0 or 1)
