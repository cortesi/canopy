-- Run cases of vi visual mode in Neovim, and write what Neovim does.
--
--   nvim --headless --clean -l run_cases.lua CASES.json RESULTS.json
--
-- CASES.json holds a list of cases. Each case has `text`, the start of the
-- cursor as `line` and `column` (both from 0, in chars), and `keys`, a list of
-- keys in Vim notation, such as "v", "<Esc>", and "<CR>". The test in
-- `tests/vim_diff.rs` writes the cases and reads the results.
--
-- RESULTS.json holds one result for each case:
--
-- * `mode`: the mode at the end, as `mode()` gives it.
-- * `text`: the lines of the buffer, joined with "\n".
-- * `cursor`: the cursor as [line, column], from 0, in chars.
-- * In visual mode, `start`: the first position of the selection, and
--   `yank`: what `y` yanks from it. Together they give the selected range.
--
-- The options match the editor: an inclusive selection, a shift of four
-- spaces, no automatic indent, and 'nostartofline', Neovim's default.

local function read(path)
  local file = assert(io.open(path, 'rb'))
  local data = file:read('*a')
  file:close()
  return data
end

local function write(path, data)
  local file = assert(io.open(path, 'wb'))
  file:write(data)
  file:close()
end

local function setup()
  vim.o.selection = 'inclusive'
  vim.o.shiftwidth = 4
  vim.o.tabstop = 4
  vim.o.softtabstop = 0
  vim.o.expandtab = true
  vim.o.shiftround = false
  vim.o.autoindent = false
  vim.o.smartindent = false
  vim.o.cindent = false
  vim.o.indentexpr = ''
  vim.o.startofline = false
  vim.o.whichwrap = 'b,s'
  vim.o.virtualedit = ''
  vim.o.wrapscan = true
  vim.o.ignorecase = false
  vim.o.smartcase = false
  vim.o.incsearch = false
  vim.o.hlsearch = false
  vim.o.magic = true
  vim.o.joinspaces = false
  vim.o.textwidth = 0
  vim.o.wrapmargin = 0
  vim.o.formatoptions = ''
  vim.o.more = false
  vim.o.shortmess = 'aoOstTWIcCFsS'
  vim.o.showmode = false
  vim.o.report = 1000000
  vim.o.undolevels = 1000
  vim.o.swapfile = false
end

local function keys(list)
  local parts = {}
  for _, key in ipairs(list) do
    parts[#parts + 1] = key == '<' and '<lt>' or key
  end
  local text = table.concat(parts)
  return vim.api.nvim_replace_termcodes(text, true, true, true)
end

local function feed(text)
  vim.api.nvim_feedkeys(text, 'ntx', false)
end

local function line_text(line)
  return vim.api.nvim_buf_get_lines(0, line, line + 1, false)[1] or ''
end

local function char_column(line, byte)
  return vim.str_utfindex(line_text(line), 'utf-32', byte, false)
end

local function before(a, b)
  return a[1] < b[1] or (a[1] == b[1] and a[2] < b[2])
end

local function run(case)
  -- A fresh buffer with no history for each case.
  local old = vim.api.nvim_get_current_buf()
  vim.cmd('enew!')
  vim.api.nvim_buf_delete(old, { force = true })
  vim.bo.undolevels = -1
  local lines = vim.split(case.text, '\n', { plain = true })
  vim.api.nvim_buf_set_lines(0, 0, -1, false, lines)
  vim.bo.undolevels = 1000
  vim.bo.modified = false

  local byte = vim.str_byteindex(lines[case.line + 1], 'utf-32', case.column, false)
  vim.api.nvim_win_set_cursor(0, { case.line + 1, byte })
  vim.fn.setreg('/', '')
  vim.fn.setreg('"', '')
  vim.v.searchforward = 1

  feed(keys(case.keys))

  local mode = vim.api.nvim_get_mode().mode
  local cursor = vim.api.nvim_win_get_cursor(0)
  local result = {
    mode = mode,
    text = table.concat(vim.api.nvim_buf_get_lines(0, 0, -1, false), '\n'),
    cursor = { cursor[1] - 1, char_column(cursor[1] - 1, cursor[2]) },
  }
  if mode == 'v' or mode == 'V' then
    local anchor = vim.fn.getpos('v')
    local ends = { cursor = result.cursor }
    ends.anchor = { anchor[2] - 1, char_column(anchor[2] - 1, anchor[3] - 1) }
    local start = before(ends.anchor, ends.cursor) and ends.anchor or ends.cursor
    if mode == 'V' then
      start = { start[1], 0 }
    end
    result.start = start
    feed('y')
    result.yank = vim.fn.getreg('"')
  end
  -- Leave any mode for the next case.
  feed(vim.api.nvim_replace_termcodes('<C-\\><C-n>', true, false, true))
  return result
end

setup()
local cases = vim.json.decode(read(arg[1]))
local results = {}
for index, case in ipairs(cases) do
  local ok, result = pcall(run, case)
  if not ok then
    result = { error = tostring(result) }
  end
  results[index] = result
end
write(arg[2], vim.json.encode(results))
vim.cmd('qa!')
