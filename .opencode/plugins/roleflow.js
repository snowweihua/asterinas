var fs = require("fs");
var path = require("path");

var ROLEFLOW_DIR = ".roleflow";
var SESSION_CONTEXT_PATH = "specs/001-rpi3-hardware-bringup/SESSION_CONTEXT.md";
var SKILL_FILE = "SKILL.md";

var ALLOWED_TOOLS = {
  planner: ["role_finish", "role_start", "workflow_status", "workflow_start", "read", "grep", "glob", "session_read", "session_search", "bash"],
  developer: ["role_finish", "role_start", "workflow_status", "read", "edit", "write", "glob", "grep", "bash"],
  builder: ["role_finish", "role_start", "workflow_status", "read", "bash"],
  verifier: ["role_finish", "role_start", "workflow_status", "read", "bash"]
};

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
  return { 
    currentRole: null, 
    roleCompleted: false,
    cycleCount: 0, 
    task: null,
    workflowActive: false
  };
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
    "tool.execute.before": async (input, output) => {
      var state = loadState();
      
      if (!state.workflowActive || !state.currentRole) {
        return;
      }

      var currentRole = state.currentRole;
      var allowedTools = ALLOWED_TOOLS[currentRole] || [];
      var toolName = input.tool;

      if (toolName === "role_finish" || toolName === "workflow_status" || toolName === "role_start" || toolName === "workflow_start") {
        return;
      }

      if (!allowedTools.includes(toolName)) {
        throw new Error(
          "[ROLE BLOCK] You are in " + currentRole.toUpperCase() + " role.\n" +
          "Allowed tools: " + allowedTools.join(", ") + "\n" +
          "You tried to use: " + toolName + " (BLOCKED)\n" +
          "Complete your " + currentRole.toUpperCase() + " task first, then call role_finish() to advance."
        );
      }
    },

    tool: {
      workflow_start: {
        description: "Start a new workflow for a specific task",
        parameters: {
          type: "object",
          properties: {
            task: { type: "string" }
          },
          required: ["task"]
        },
        async execute(args, context) {
          var task = args.task;
          var state = loadState();
          state.task = task;
          state.currentRole = "planner";
          state.roleCompleted = false;
          state.workflowActive = true;
          state.cycleCount = 1;
          saveState(state);

          var skillContent = loadSkillFile();
          var roleChain = parseRoleChain(skillContent);
          var template = loadRoleTemplate("planner");
          var nextRole = roleChain["planner"] ? roleChain["planner"].next : null;
          var allowedTools = ALLOWED_TOOLS["planner"] || [];

          return {
            workflow: "started",
            task: task,
            currentRole: "planner",
            nextRole: nextRole,
            allowedTools: allowedTools,
            contextSummary: "=== WORKFLOW STARTED ===\n\n" +
              "Task: " + task + "\n\n" +
              "You are PLANNER (first role)\n\n" +
              "## ALLOWED TOOLS (you can ONLY use these):\n" +
              allowedTools.join(", ") + "\n\n" +
              template + "\n\n" +
              "=== Complete PLANNER task using ONLY the allowed tools, then call role_finish() ==="
          };
        }
      },

      role_start: {
        description: "Start a role in the workflow - validates role order and sets context",
        parameters: {
          type: "object",
          properties: {
            role: { type: "string", enum: ["planner", "developer", "builder", "verifier"] }
          },
          required: ["role"]
        },
        async execute(args, context) {
          var requestedRole = args.role;
          var state = loadState();
          var skillContent = loadSkillFile();
          var roleChain = parseRoleChain(skillContent);

          if (!state.workflowActive) {
            throw new Error("[ERROR] No active workflow. Call workflow_start({task: \"T030\"}) first.");
          }

          if (state.currentRole !== requestedRole) {
            var expectedRole = state.currentRole;
            throw new Error(
              "[TRANSITION BLOCK] Cannot switch to " + requestedRole.toUpperCase() + ".\n" +
              "Current role is " + (expectedRole ? expectedRole.toUpperCase() : "NONE") + ".\n" +
              "Must complete " + (expectedRole ? expectedRole.toUpperCase() : "CURRENT") + " task first.\n" +
              "Call role_finish() to advance to next role."
            );
          }

          if (!state.roleCompleted) {
            throw new Error(
              "[ROLE INCOMPLETE] You are still in " + state.currentRole.toUpperCase() + " role.\n" +
              "You must call role_finish() to complete this role before continuing.\n" +
              "Complete your task, then call: role_finish({summary: '...', progress: '...'})"
            );
          }

          var template = loadRoleTemplate(requestedRole);
          var nextRole = roleChain[requestedRole] ? roleChain[requestedRole].next : null;
          var allowedTools = ALLOWED_TOOLS[requestedRole] || [];

          return {
            role: requestedRole,
            template: template,
            nextRole: nextRole,
            task: state.task,
            allowedTools: allowedTools,
            contextSummary: "=== ROLE CONTEXT ===\n\n" +
              "You are " + requestedRole.toUpperCase() + "\n\n" +
              "## ALLOWED TOOLS (you can ONLY use these):\n" +
              allowedTools.join(", ") + "\n\n" +
              template + "\n\n" +
              "## Your Task\n" +
              (state.task || "No active task") + "\n\n" +
              "=== Complete your task using ONLY the allowed tools, then call role_finish() ==="
          };
        }
      },

      role_finish: {
        description: "Finish current role and advance to next role",
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

          if (!currentRole) {
            throw new Error("[ERROR] No active role. Call role_start() or workflow_start() first.");
          }

          state.lastSummary = summary;
          state.lastProgress = progress;
          state.roleCompleted = true;

          if (progress === "finished") {
            state.workflowActive = false;
            saveState(state);
            return {
              action: "STOP",
              summary: summary,
              message: "Workflow completed successfully."
            };
          }

          if (progress === "user_action_needed") {
            saveState(state);
            return {
              action: "USER_ACTION_NEEDED",
              summary: summary,
              message: "Waiting for user action. Workflow paused.",
              nextRole: "verifier",
              paused: true
            };
          }

          if (progress === "build_failed") {
            var nextRole = "developer";
            state.currentRole = nextRole;
            state.roleCompleted = false;
            state.cycleCount++;
            saveState(state);

            var template = loadRoleTemplate(nextRole);
            var allowedTools = ALLOWED_TOOLS[nextRole] || [];

            return {
              action: "BUILD_FAILED",
              summary: summary,
              message: "Build failed. Routing to Developer to fix.",
              nextRole: nextRole,
              allowedTools: allowedTools,
              contextSummary: "=== ROLE CONTEXT ===\n\n" +
                "You are " + nextRole.toUpperCase() + "\n\n" +
                "## ALLOWED TOOLS (you can ONLY use these):\n" +
                allowedTools.join(", ") + "\n\n" +
                template + "\n\n" +
                "## Error to fix:\n" +
                summary + "\n\n" +
                "=== Fix the build error using ONLY the allowed tools, then call role_finish() ==="
            };
          }

          var skillContent = loadSkillFile();
          var nextRole = getNextRole(currentRole, skillContent);
          state.currentRole = nextRole;
          state.roleCompleted = false;
          state.cycleCount++;
          saveState(state);

          var template = loadRoleTemplate(nextRole);
          var allowedTools = ALLOWED_TOOLS[nextRole] || [];

          return {
            action: "CONTINUE",
            summary: summary,
            message: "Role completed. Advancing to " + nextRole.toUpperCase(),
            nextRole: nextRole,
            allowedTools: allowedTools,
            contextSummary: "=== ROLE CONTEXT ===\n\n" +
              "You are " + nextRole.toUpperCase() + "\n\n" +
              "## ALLOWED TOOLS (you can ONLY use these):\n" +
              allowedTools.join(", ") + "\n\n" +
              template + "\n\n" +
              "## Previous role result:\n" +
              summary + "\n\n" +
              "=== Complete your task using ONLY the allowed tools, then call role_finish() ==="
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
            roleCompleted: state.roleCompleted,
            workflowActive: state.workflowActive,
            cycleCount: state.cycleCount,
            lastSummary: state.lastSummary,
            lastProgress: state.lastProgress,
            allowedTools: state.currentRole ? (ALLOWED_TOOLS[state.currentRole] || []) : []
          };
        }
      },
    }
  };
};
