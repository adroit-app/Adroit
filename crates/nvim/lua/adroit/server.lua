-- What the prompt offers, served as a language server's completion inside
-- its Neovim, so that whichever completion the configuration runs, Neovim's
-- own or a plugin's, shows it. Every answer is incomplete: what matches is
-- found anew as more is typed, by any part of a path rather than its start.
local offer_rules = require("adroit.offer")

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
--- gives unless `withheld` says otherwise by then.
function M.serve(complete, withheld)
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
      ["textDocument/completion"] = function(params, done)
        return complete(params, done)
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
        local answered = false
        local function done(result)
          if answered then
            return
          end
          answered = true
          if not (method == "textDocument/completion" and withheld and withheld()) then
            callback(nil, result)
          end
          if notify_reply then
            notify_reply(mine)
          end
        end
        vim.schedule(function()
          if answer then
            local result = answer(params, done)
            if result ~= nil then
              done(result)
            end
          else
            callback({ code = -32601, message = M.NAME .. " serves no " .. method })
            if notify_reply then
              notify_reply(mine)
            end
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

--- Whether the menu shows a candidate already put in the text of `buf`, where
--- nothing is offered. Neovim shows every answer in place of the menu, so an
--- empty one, or one to what was typed before, would close it.
local function chosen_in_menu(buf)
  if vim.api.nvim_get_current_buf() ~= buf or vim.fn.pumvisible() == 0 then
    return false
  end
  return not offer_rules.offers(vim.api.nvim_get_current_line():sub(1, vim.fn.col(".") - 1))
end

--- Attach a language server that answers completion with what `offer`
--- gives for the text of a line of `buf` up to the cursor, to `buf`.
function M.start(buf, offer)
  return vim.lsp.start({
    name = M.NAME,
    cmd = M.serve(function(params, done)
      local line, col = params.position.line, params.position.character
      local before = (vim.api.nvim_buf_get_lines(buf, line, line + 1, false)[1] or ""):sub(1, col)
      local function finish(offered)
        return offered and M.items(offered, before, params.position) or { isIncomplete = true, items = {} }
      end
      local offered, pending = offer(before, function(result)
        done(finish(result))
      end)
      if pending then
        return nil
      end
      return finish(offered)
    end, function()
      return chosen_in_menu(buf)
    end),
  }, { bufnr = buf })
end

return M
