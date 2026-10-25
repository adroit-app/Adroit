-- Tells the window whenever the user's configuration is saved: `init.lua`,
-- or a file under the configuration folder's `lua/`. The window starts this
-- Neovim again rather than it reading the configuration again, as plugin
-- managers such as lazy.nvim cannot be set up twice in one Neovim.
local function watch_configuration()
  local init = vim.env.MYVIMRC
  if not init or init == "" then
    init = vim.fs.joinpath(vim.fn.stdpath("config"), "init.lua")
  end
  local dir = vim.fs.dirname(init)
  local name = vim.fs.basename(init)
  -- A folder that is not there cannot be watched, and a configuration first
  -- written after starting is to be told of too. Another Neovim may be making
  -- it at the same moment.
  pcall(vim.fn.mkdir, dir, "p")
  local timer = vim.uv.new_timer()
  -- Each folder is watched on its own, as only macOS and Windows watch one
  -- with all the folders under it.
  local watched = {}
  local watch_lua
  local function watch(path, prefix)
    if watched[path] then
      return
    end
    local watcher = vim.uv.new_fs_event()
    local ok = watcher
      and watcher:start(path, {}, function(err, file)
        if err or not file then
          return
        end
        local rel = prefix .. file
        if rel ~= name and rel ~= "lua" and not vim.startswith(rel, "lua/") then
          return
        end
        -- An editor's save comes as several events: tell once they stop. A
        -- folder made meanwhile is watched from then on.
        timer:start(
          100,
          0,
          vim.schedule_wrap(function()
            watch_lua()
            vim.rpcnotify(vim.g.adroit_channel, "adroit_configuration_saved")
          end)
        )
      end)
    if ok then
      watched[path] = watcher
    end
  end
  watch_lua = function()
    local lua = vim.fs.joinpath(dir, "lua")
    if not vim.uv.fs_stat(lua) then
      return
    end
    watch(lua, "lua/")
    for sub, kind in vim.fs.dir(lua, { depth = math.huge }) do
      if kind == "directory" then
        watch(vim.fs.joinpath(lua, sub), "lua/" .. sub .. "/")
      end
    end
  end
  watch(dir, "")
  watch_lua()
end
