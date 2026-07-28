import type { Plugin } from "@opencode-ai/plugin"
import * as fs from "fs/promises"
import * as path from "path"

export const PreserveRulesPlugin: Plugin = async (ctx) => {
  return {
    "experimental.session.compacting": async (input, output) => {
      try {
        // Define the path to your project's AGENTS.md
        // Note: modify the path resolution depending on where your project executes
        const agentsPath = path.resolve(process.cwd(), "AGENTS.md")
        
        // Read the file directly via Node fs (since MCP/Agent tools are locked)
        const agentsContent = await fs.readFile(agentsPath, "utf-8")
        
        // Inject the content so the compaction LLM registers it during summary building
        output.context.push(`
          CRITICAL: Do NOT summarize, generalize, or omit the following rules. 
          They dictate your active routing and operational capabilities. Ensure 
          they are explicitly preserved in the final continuation checkpoint:
          
          ${agentsContent}
        `)
      } catch (error) {
        // Fallback if the file doesn't exist in the current directory
        console.error("Failed to inject AGENTS.md during compaction:", error)
      }
    },
  }
}

