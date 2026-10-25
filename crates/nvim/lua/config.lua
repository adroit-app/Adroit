-- Reads the user's configuration again whenever it is saved: `init.lua`, or
-- a Lua module under the configuration folder's `lua/`, which is forgotten
-- first so that `require` reads it anew. The filetype plugins of `buf` run
-- again too, as what the configuration sets for its filetype would otherwise
-- wait for the next start, and then `after`, for what has to come after them.
-- A configuration that fails is told of, and what was read before stays.
local function watch_configuration(buf, after)
  local init = vim.env.MYVIMRC
  if not init or init == "" then
    init = vim.fs.joinpath(vim.fn.stdpath("config"), "init.lua")
  end
  local dir = vim.fs.dirname(init)
  local name = vim.fs.basename(init)
  -- A folder that is not there cannot be watched, and a configuration first
  -- written after starting is to be read too. Another Neovim may be making
  -- it at the same moment.
  pcall(vim.fn.mkdir, dir, "p")
  local function forget_modules()
    for module in pairs(package.loaded) do
      local file = module:gsub("%.", "/")
      for _, rel in ipairs({ "lua/" .. file .. ".lua", "lua/" .. file .. "/init.lua" }) do
        if vim.uv.fs_stat(vim.fs.joinpath(dir, rel)) then
          package.loaded[module] = nil
        end
      end
    end
  end
  local function read()
    if not vim.uv.fs_stat(init) then
      return
    end
    forget_modules()
    local ok, err = pcall(vim.cmd.source, init)
    if not ok then
      vim.notify("adroit: cannot read " .. init .. ": " .. tostring(err), vim.log.levels.ERROR)
      return
    end
    if vim.api.nvim_buf_is_valid(buf) then
      vim.api.nvim_buf_call(buf, function()
        vim.cmd("doautocmd FileType " .. vim.bo[buf].filetype)
      end)
    end
    if after then
      after()
    end
  end
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
        -- An editor's save comes as several events: read once they stop. A
        -- folder made meanwhile is watched from then on.
        timer:start(
          100,
          0,
          vim.schedule_wrap(function()
            watch_lua()
            read()
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
