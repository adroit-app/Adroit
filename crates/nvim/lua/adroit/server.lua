-- What the prompt offers, served as a language server's completion inside
-- its Neovim, so that whichever completion the configuration runs, Neovim's
-- own or a plugin's, shows it. Every answer is incomplete: what matches is
-- found anew as more is typed, by any part of a path rather than its start.
local M = {}

M.NAME = "adroit"
M.TRIGGERS = { "@", "/", "$" }

--- The completion items for `offered`, what the prompt offers at `position`
--- of a line whose text up to it is `before`.
function M.items(offered, before, position)
  local out = {}
  for i, o in ipairs(offered.items) do
    out[i] = {
      label = o.abbr,
      filterText = before:sub(offered.start),
      sortText = string.format("%05d", i),
      detail = o.menu,
      documentation = o.info ~= "" and o.info or nil,
      textEdit = {
        range = {
          start = { line = position.line, character = offered.start - 1 },
          ["end"] = position,
        },
        newText = o.word,
      },
    }
  end
  return { isIncomplete = true, items = out }
end

--- A language server for `cmd`, whose completion asks `complete` with the
--- text of the line up to the cursor and the position, and answers what it
--- gives.
function M.serve(complete)
  return function(dispatchers)
    local closing, id = false, 0
    local answers = {
      initialize = function()
        return {
          capabilities = {
            positionEncoding = "utf-8",
            completionProvider = { triggerCharacters = M.TRIGGERS },
          },
          serverInfo = { name = M.NAME },
        }
      end,
      ["textDocument/completion"] = function(params)
        return complete(params)
      end,
      shutdown = function()
        return vim.NIL
      end,
    }
    return {
      request = function(method, params, callback, notify_reply)
        id = id + 1
        local mine = id
        local answer = answers[method]
        vim.schedule(function()
          if answer then
            callback(nil, answer(params))
          else
            callback({ code = -32601, message = M.NAME .. " serves no " .. method })
          end
          if notify_reply then
            notify_reply(mine)
          end
        end)
        return true, mine
      end,
      notify = function(method)
        if method == "exit" then
          closing = true
          dispatchers.on_exit(0, 15)
        end
        return true
      end,
      is_closing = function()
        return closing
      end,
      terminate = function()
        closing = true
      end,
    }
  end
end

--- Attach a language server that answers completion with what `offer`
--- gives for the text of a line of `buf` up to the cursor, to `buf`.
function M.start(buf, offer)
  return vim.lsp.start({
    name = M.NAME,
    cmd = M.serve(function(params)
      local line, col = params.position.line, params.position.character
      local before = (vim.api.nvim_buf_get_lines(buf, line, line + 1, false)[1] or ""):sub(1, col)
      local offered = offer(before)
      if not offered then
        return { isIncomplete = true, items = {} }
      end
      return M.items(offered, before, params.position)
    end),
  }, { bufnr = buf })
end

return M
