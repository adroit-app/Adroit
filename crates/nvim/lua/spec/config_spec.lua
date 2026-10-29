local config = require("adroit.config")

local function isolated(body)
  local root = vim.fn.tempname()
  local stdpath, mkdir = vim.fn.stdpath, vim.uv.fs_mkdir
  vim.fn.stdpath = function(kind)
    eq("data", kind)
    return root .. "/shared/adroit/prompt/nvim"
  end
  local ok, err = xpcall(function()
    body(root, mkdir)
  end, debug.traceback)
  vim.fn.stdpath, vim.uv.fs_mkdir = stdpath, mkdir
  vim.fn.delete(root, "rf")
  if not ok then
    error(err)
  end
end

describe("embedded Neovim startup", function()
  it("creates its data directory before a plugin needs its manifest", function()
    isolated(function(root)
      config.prepare()
      eq(1, vim.fn.isdirectory(root .. "/shared/adroit/prompt/nvim"))
      config.prepare()
    end)
  end)

  it("accepts a shared ancestor created by another Neovim meanwhile", function()
    isolated(function(root, mkdir)
      vim.uv.fs_mkdir = function(path, mode)
        if path == root .. "/shared" then
          assert(mkdir(path, mode))
          return nil, "EEXIST: file already exists", "EEXIST"
        end
        return mkdir(path, mode)
      end
      config.prepare()
      eq(1, vim.fn.isdirectory(root .. "/shared/adroit/prompt/nvim"))
    end)
  end)

  it("reports a file blocking the data directory", function()
    isolated(function(root)
      vim.fn.mkdir(root, "p")
      vim.fn.writefile({ "blocking file" }, root .. "/shared")
      local ok = pcall(config.prepare)
      eq(false, ok)
      eq({ "blocking file" }, vim.fn.readfile(root .. "/shared"))
    end)
  end)
end)
