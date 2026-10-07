import { execFileSync } from "node:child_process";

export function getLastCommitDate(filePath: string): Date | undefined {
  try {
    const output = execFileSync("git", ["log", "-1", "--format=%cI", "--", filePath], {
      cwd: process.cwd(),
      encoding: "utf-8",
    }).trim();
    return output ? new Date(output) : undefined;
  } catch {
    return undefined;
  }
}
