-- 1. Setup Lazy.nvim
local lazypath = vim.fn.stdpath("data") .. "/lazy/lazy.nvim"
if not vim.loop.fs_stat(lazypath) then
  vim.fn.system({ "git", "clone", "--filter=blob:none", "https://github.com/folke/lazy.nvim.git", "--branch=stable", lazypath })
end
vim.opt.rtp:prepend(lazypath)

-- 2. Basic Settings
vim.g.mapleader = " "
vim.opt.termguicolors = true
vim.opt.number = true
vim.opt.relativenumber = true
vim.opt.mouse = "a" -- Enable mouse support for the tree

-- 3. Plugins
require("lazy").setup({
  -- THEME
  {
    "catppuccin/nvim",
    name = "catppuccin",
    priority = 1000,
    config = function()
      require("catppuccin").setup({ transparent_background = true })
      vim.cmd.colorscheme "catppuccin"
    end,
  },

  -- TELESCOPE
  {
    'nvim-telescope/telescope.nvim', tag = '0.1.8',
    dependencies = { 'nvim-lua/plenary.nvim' },
    config = function() require("telescope").setup({}) end,
  },

  -- STATUSLINE
  {
    'nvim-lualine/lualine.nvim',
    dependencies = { 'nvim-tree/nvim-web-devicons' },
    config = function() require('lualine').setup({ options = { theme = 'catppuccin' } }) end
  },

  -- FILE TREE
  {
    'nvim-tree/nvim-tree.lua',
    dependencies = { 'nvim-tree/nvim-web-devicons' },
    config = function() require("nvim-tree").setup({}) end
  },
  --toggleterm aka the runner
  {
    'akinsho/toggleterm.nvim',
    version = "*",
    config = function()
      require("toggleterm").setup({
        size = 20,
        open_mapping = [[<c-\>]], -- Ctrl + \ toggles the main terminal
        shade_terminals = false,
        direction = 'float', -- This makes it a cool floating window
        float_opts = {
          border = 'curved',
        },
      })
    end
  },
  {
    "windwp/nvim-autopairs",
    event = "InsertEnter",
    config = true
  },
  {
    "williamboman/mason.nvim",
    build = ":MasonUpdate",
    config = true,
  },

  -- LSP
  {
    "neovim/nvim-lspconfig",
    dependencies = {
      "williamboman/mason-lspconfig.nvim",
      "hrsh7th/cmp-nvim-lsp",
    },
    config = function()
      local capabilities = require('cmp_nvim_lsp').default_capabilities()

      require("mason-lspconfig").setup({
        ensure_installed = { "lua_ls", "bashls", "pyright", "gopls" },
        handlers = {
          function(server_name)
            require("lspconfig")[server_name].setup({ capabilities = capabilities })
          end,
        },
      })
    end,
  },

  -- opencode.nvim
  {
    "nickjvandyke/opencode.nvim",
    version = "*",
    config = function()
      vim.g.opencode_opts = {}

      vim.keymap.set({ "n", "x" }, "<leader>oa", function() require("opencode").ask("@this: ", { submit = true }) end, { desc = "Ask opencode" })
      vim.keymap.set({ "n", "x" }, "<leader>os", function() require("opencode").select() end, { desc = "Select opencode" })
      vim.keymap.set({ "n", "t" }, "<leader>ot", function() require("opencode").toggle() end, { desc = "Toggle opencode" })
    end,
  },

  -- Autocomplete
  {
    "hrsh7th/nvim-cmp",
    dependencies = {
      "hrsh7th/cmp-nvim-lsp",
      "L3MON4D3/LuaSnip",
      "saadparwaiz1/cmp_luasnip",
    },
    config = function()
      local cmp = require("cmp")
      local luasnip = require("luasnip")

      cmp.setup({
        snippet = {
          expand = function(args)
            luasnip.lsp_expand(args.body)
          end,
        },
        mapping = cmp.mapping.preset.insert({
          ["<C-b>"] = cmp.mapping.scroll_docs(-4),
          ["<C-f>"] = cmp.mapping.scroll_docs(4),
          ["<C-Space>"] = cmp.mapping.complete(),
          ["<C-e>"] = cmp.mapping.abort(),
          ["<CR>"] = cmp.mapping.confirm({ select = true }),
          ["<Tab>"] = cmp.mapping(function(fallback)
            if cmp.visible() then
              cmp.select_next_item()
            elseif luasnip.expand_or_jumpable() then
              luasnip.expand_or_jump()
            else
              fallback()
            end
          end, { "i", "s" }),
          ["<S-Tab>"] = cmp.mapping(function(fallback)
            if cmp.visible() then
              cmp.select_prev_item()
            elseif luasnip.jumpable(-1) then
              luasnip.jump(-1)
            else
              fallback()
            end
          end, { "i", "s" }),
        }),
        sources = cmp.config.sources({
          { name = "nvim_lsp" },
          { name = "luasnip" },
        }, {
          { name = "buffer" },
        }),
      })
    end,
  },

})

-- 4. The Bulletproof Transparency
local function clear_bg()
  local groups = { 
    "Normal", "NormalNC", "SignColumn", "MsgArea", 
    "TelescopeNormal", "TelescopeBorder", "NvimTreeNormal", "NvimTreeNormalNC",
    "ToggleTerm1FloatNormal", "ToggleTerm1FloatBorder"
  }
  for _, group in ipairs(groups) do
    vim.api.nvim_set_hl(0, group, { bg = "none", ctermbg = "none" })
  end
end

vim.api.nvim_create_autocmd("ColorScheme", { callback = clear_bg })
clear_bg()

-- 5. Keymaps
local builtin = require('telescope.builtin')
vim.keymap.set('n', '<leader>ff', builtin.find_files, {})
vim.keymap.set('n', '<leader>e', ':NvimTreeToggle<CR>', { silent = true }) -- Toggle File Tree

vim.keymap.set('n', '<leader>r', function()
  vim.cmd("write")
  local file_ext = vim.bo.filetype
  if file_ext == "python" then
    vim.cmd("TermExec cmd='python3 %'")
  elseif file_ext == "sh" then
    vim.cmd("TermExec cmd='bash %'")
  elseif file_ext == "go" then
    -- 'go run' compiles and runs the file in one step
    vim.cmd("TermExec cmd='go run %'")
  else
    print("No runner configured for " .. file_ext)
  end
end)
