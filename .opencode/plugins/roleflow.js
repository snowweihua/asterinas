var fs = require("fs");
var path = require("path");

var ROLEFLOW_DIR = ".roleflow";
var SESSION_CONTEXT_PATH = "specs/001-rpi3-hardware-bringup/SESSION_CONTEXT.md";
var SKILL_FILE = "SKILL.md";

function ensureDir(dir) {
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
}

function getCurrentTask() {
  var sessionPath = path.join(process.cwd(), SESSION_CONTEXT_PATH);
  if (fs.existsSync(sessionPath)) {
    var content = fs.readFileSync(sessionPath, "utf-8");
    var match = content.match(/Current Task:\s*(\S+)/);
    return match ? match[1] : null;
  }
  return null;
}

function loadRoleTemplate(role) {
  var templatePath = path.join(process.cwd(), ROLEFLOW_DIR, role + ".md");
  if (fs.existsSync(templatePath)) {
    return fs.readFileSync(templatePath, "utf-8");
  }
  return "";
}

function loadSkillFile() {
  var skillPath = path.join(process.cwd(), ROLEFLOW_DIR, SKILL_FILE);
  if (fs.existsSync(skillPath)) {
    return fs.readFileSync(skillPath, "utf-8");
  }
  return "";
}

function parseRoleChain(skillContent) {
  var roles = {};
  var lines = skillContent.split("\n");
  var parsingTable = false;
  var headerParsed = false;

  for (var i = 0; i < lines.length; i++) {
    var line = lines[i].trim();

    if (line === "| Role | Next | Responsibilities |") {
      parsingTable = true;
      headerParsed = true;
      continue;
    }

    if (parsingTable && headerParsed) {
      if (line === "|" || line.startsWith("---")) {
        continue;
      }

      var parts = line.split("|");
      if (parts.length >= 3) {
        var role = parts[1].trim();
        var nextRole = parts[2].trim();

        if (role && nextRole && role !== "" && nextRole !== "") {
          roles[role] = { next: nextRole };
        }
      }
    }
  }

  return roles;
}

function getStateFilePath() {
  return path.join(process.cwd(), ".github/agent_state/roleflow_state.json");
}

function loadState() {
  var statePath = getStateFilePath();
  if (fs.existsSync(statePath)) {
    return JSON.parse(fs.readFileSync(statePath, "utf-8"));
  }
  return { currentRole: "planner", cycleCount: 0, task: null };
}

function saveState(state) {
  var stateDir = path.join(process.cwd(), ".github/agent_state");
  ensureDir(stateDir);
  fs.writeFileSync(getStateFilePath(), JSON.stringify(state, null, 2));
}

function getNextRole(currentRole, skillContent) {
  var roleChain = parseRoleChain(skillContent);
  return roleChain[currentRole] ? roleChain[currentRole].next : null;
}

exports.RoleflowPlugin = async function(ctx) {
  return {
    tool: {
      role_start: {
        description: "Start a role - loads ONLY that role's context, isolating from other roles",
        parameters: {
          type: "object",
          properties: {
            role: { type: "string", enum: ["planner", "developer", "builder", "verifier"] }
          },
          required: ["role"]
        },
        async execute(args, context) {
          var role = args.role;
          var template = loadRoleTemplate(role);
          var skillContent = loadSkillFile();
          var roleChain = parseRoleChain(skillContent);
          var nextRole = roleChain[role] ? roleChain[role].next : null;

          var state = loadState();
          state.currentRole = role;
          state.lastRole = role;
          saveState(state);

          var task = state.task || getCurrentTask();

          return {
            role: role,
            template: template,
            nextRole: nextRole,
            task: task,
            contextSummary: "=== ROLE CONTEXT (ONLY READ THIS) ===\n\n" +
              "You are " + role.toUpperCase() + "\n\n" +
              template + "\n\n" +
              "## Your Task\n" +
              (task || "No active task") + "\n\n" +
              "=== DO NOT read previous role outputs unless they contain your task ==="
          };
        }
      },

      role_finish: {
        description: "Finish current role and transition to next role",
        parameters: {
          type: "object",
          properties: {
            summary: { type: "string" },
            progress: { type: "string", enum: ["progress", "no_progress", "finished", "build_failed", "user_action_needed"] }
          },
          required: ["summary", "progress"]
        },
        async execute(args, context) {
          var summary = args.summary;
          var progress = args.progress;

          var state = loadState();
          var currentRole = state.currentRole;

          state.lastSummary = summary;
          state.lastProgress = progress;
          if (progress !== "finished" && progress !== "user_action_needed") {
            state.cycleCount++;
          }
          saveState(state);

          if (progress === "finished") {
            return {
              action: "STOP",
              summary: summary,
              message: "Workflow completed"
            };
          }

          if (progress === "user_action_needed") {
            return {
              action: "USER_ACTION_NEEDED",
              summary: summary,
              message: "Waiting for user to power on/reset board. Workflow paused. Run /roleflow continue when ready.",
              nextRole: "verifier",
              paused: true
            };
          }

          if (progress === "build_failed") {
            return {
              action: "BUILD_FAILED",
              summary: summary,
              message: "Build failed - routing to Developer to fix error",
              nextRole: "developer",
              errorDetail: summary
            };
          }

          if (progress === "progress") {
            var skillContent = loadSkillFile();
            var nextRole = getNextRole(currentRole, skillContent);
            return {
              action: "CONTINUE",
              summary: summary,
              message: "Progress made - continuing to next role",
              nextRole: nextRole
            };
          }

          return {
            action: "CONTINUE_RESEARCH",
            summary: summary,
            message: "No progress - need more research/analysis",
            nextRole: "planner"
          };
        }
      },

      workflow_status: {
        description: "Get current workflow status",
        parameters: {
          type: "object",
          properties: {}
        },
        async execute(args, context) {
          var skillContent = loadSkillFile();
          var roleChain = parseRoleChain(skillContent);
          var state = loadState();
          return {
            roleChain: roleChain,
            currentTask: state.task || getCurrentTask(),
            currentRole: state.currentRole,
            cycleCount: state.cycleCount,
            lastSummary: state.lastSummary,
            lastProgress: state.lastProgress,
            paused: state.lastProgress === "user_action_needed"
          };
        }
      },
    }
  };
};
