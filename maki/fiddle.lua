local function register_fiddle(root, skills)
  local function run_skill(name, args)
    local request = args ~= "" and args or "Continue using this skill for the current task."
    local prompt = table.concat({
      "Execute the Fiddle skill at " .. root .. "/skills/" .. name .. "/SKILL.md.",
      "Read that file and its relative references from " .. root .. ".",
      "Do not merely summarize the skill; follow its instructions for the user's request.",
      "",
      "User request:",
      request,
    }, "\n")

    local state, prompt_err = maki.session.prompt(prompt)
    if not state then
      maki.ui.flash("Fiddle: " .. (prompt_err or "could not send prompt"))
    end
  end

  for _, name in ipairs(skills) do
    maki.api.register_command({
      name = "/fiddle:" .. name,
      description = "Run the Fiddle " .. name .. " skill.",
      nargs = "*",
      handler = function(opts)
        run_skill(name, opts.args or "")
      end,
    })
  end
end
