-- nvim: apply a palette's colours, and follow the file when it changes
local M = {}

local uv = vim.uv or vim.loop
local path = vim.fn.expand("~/.config/colors/palette.json")

---@class Palette
---@field name string
---@field colours table<string, string>

local function read_palette(file)
  local fd, err = io.open(file, "r")
  if not fd then
    return nil, "can't open " .. file .. ": " .. tostring(err)
  end
  local ok, data = pcall(vim.json.decode, fd:read("*a"))
  fd:close()
  if not ok or type(data) ~= "table" then
    return nil, "not a palette: " .. file
  end
  return data
end

local groups = {
  Normal = { fg = "fg0", bg = "bg0" },
  Comment = { fg = "grey1", italic = true },
  String = { fg = "green" },
  Number = { fg = "orange" },
  Keyword = { fg = "purple", bold = true },
  Function = { fg = "blue" },
  Type = { fg = "yellow" },
  Visual = { bg = "bg_visual_blue" },
}

function M.apply(p)
  vim.cmd("highlight clear")
  for group, spec in pairs(groups) do
    local hl = vim.deepcopy(spec)
    hl.fg = spec.fg and p.colours[spec.fg] or nil
    hl.bg = spec.bg and p.colours[spec.bg] or nil
    vim.api.nvim_set_hl(0, group, hl)
  end
  vim.g.colors_name = p.name or "palette"
end

function M.setup()
  local p, err = read_palette(path)
  if not p then
    return vim.notify(err, vim.log.levels.WARN)
  end
  M.apply(p)
  local watcher = uv.new_fs_event()
  watcher:start(path, {}, vim.schedule_wrap(function()
    local fresh = read_palette(path)
    if fresh then M.apply(fresh) end
  end))
end

return M
