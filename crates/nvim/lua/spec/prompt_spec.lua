local prompt = require("adroit.prompt")

describe("path answers", function()
  it("discards an old root's answer even when its typed path is the same", function()
    local sent, notify = {}, vim.rpcnotify
    vim.rpcnotify = function(_, name, id, root, typed)
      if name == "adroit_paths_wanted" then
        sent[#sent + 1] = { id = id, root = root, typed = typed }
      end
    end
    local ok, err = xpcall(function()
      local old, current = "waiting", "waiting"
      prompt.root("/host/old")
      prompt.offer(1, "@src/", function(result)
        old = result
      end)
      prompt.root("/host/current")
      prompt.offer(1, "@src/", function(result)
        current = result
      end)
      eq(nil, old)
      eq(2, #sent)
      eq("/host/current", sent[2].root)
      eq("src/", sent[2].typed)
      prompt.paths(sent[1].id, { { word = "src/old.rs", menu = "file" } })
      eq("waiting", current)
      prompt.paths(sent[2].id, { { word = "src/current.rs", menu = "file" } })
      eq("src/current.rs ", current.items[1].word)
      prompt.paths(sent[1].id, { { word = "src/old.rs", menu = "file" } })
      eq("src/current.rs ", prompt.offer(1, "@src/").items[1].word)
    end, debug.traceback)
    vim.rpcnotify = notify
    if not ok then
      error(err)
    end
  end)
end)
