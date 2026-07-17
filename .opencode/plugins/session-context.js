import fs from "fs"
import { join, dirname } from "path"
import { fileURLToPath } from "url"

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)
const REPO_ROOT = join(__dirname, "..", "..")

const QUICKSTART_PATH = join(REPO_ROOT, "specs/001-rpi3-hardware-bringup/quickstart.md")
let quickstartContent = null
try {
  quickstartContent = fs.readFileSync(QUICKSTART_PATH, "utf-8")
} catch (e) {
  console.info("[session-context] Failed to read quickstart.md: " + e.message)
}

export default async function () {
  return {
    "experimental.session.compacting": async (_, output) => {
      if (quickstartContent) {
        output.context.push(`=== RPI3 HARDWARE BRINGUP QUICKSTART ===\n${quickstartContent}\n=== END QUICKSTART ===`)
      }
    },
  }
}
