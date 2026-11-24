import type {
  GitAuth,
  GitStatus,
  SyncSettings,
} from "../api";
import { coded, gitState, session } from "./store";
import { configureSync } from "./sync";
import type { Handlers } from "./handlers";

export function gitStatus(): GitStatus {
  if (session.remote?.provider.kind !== "git") {
    throw coded("syncNotConfigured", "sync is not set up");
  }
  return structuredClone(gitState);
}

export function gitMove(paths: string[], stage: boolean): GitStatus {
  gitStatus();
  for (const change of gitState.changes) {
    if (!paths.includes(change.path)) continue;
    if (stage && change.unstaged) {
      change.staged = change.unstaged === "untracked" ? "added" : change.unstaged;
      change.unstaged = null;
    } else if (!stage && change.staged) {
      change.unstaged = change.staged === "added" ? "untracked" : change.staged;
      change.staged = null;
    }
  }
  return gitStatus();
}

export function gitDiscard(path: string, staged: boolean): GitStatus {
  gitStatus();
  gitState.changes = gitState.changes.filter((change) => {
    if (change.path !== path) return true;
    if (staged) return false;
    change.unstaged = null;
    return change.staged !== null;
  });
  return gitStatus();
}

export function gitDiff(path: string, staged: boolean): string {
  const change = gitStatus().changes.find((entry) => entry.path === path);
  const kind = staged ? change?.staged : change?.unstaged;
  if (!kind) return "";
  if (path.endsWith(".png")) {
    return `diff --git a/${path} b/${path}\nnew file mode 100644\nBinary files /dev/null and b/${path} differ\n`;
  }
  const head = `diff --git a/${path} b/${path}\n`;
  if (kind === "added" || kind === "untracked") {
    return `${head}new file mode 100644\n--- /dev/null\n+++ b/${path}\n@@ -0,0 +1,3 @@\n+# New thought\n+\n+A draft of an idea.\n`;
  }
  return (
    `${head}--- a/${path}\n+++ b/${path}\n@@ -1,4 +1,5 @@\n # Contents\n \n-Old line.\n+New line.\n+One more.\n The end.\n`
  );
}

export function connectGit(args: Record<string, unknown> | undefined): SyncSettings {
  if (!session.appSettings.gitPath) {
    throw coded("gitPathNotSet", "the path to git is not set: set it in the settings");
  }
  const url = (args?.url as string | null) ?? (session.remote?.provider.kind === "git" ? session.remote.provider.url : null);
  if (!url) throw coded("noGitUrl", "sync settings: no repository address is set");
  if (/^https?:\/\/[^/]*@/.test(url)) {
    throw coded("gitUrlCredentials", "the repository address contains a login or a token");
  }
  const auth = args?.auth as GitAuth;
  const token = Boolean(args?.token);
  if (auth === "token" && !token && !session.remote?.hasSecret) {
    throw coded("noGitToken", "repository token not found");
  }
  return configureSync(
    {
      provider: { kind: "git", url, auth, env: (args?.env as string | null) ?? null },
      policy: "keepBoth",
      autoSyncMinutes: 0,
    },
    token,
  );
}


export const gitHandlers = {
  git_locate: () => {
    return "/opt/homebrew/bin/git";
  },

  git_version: (args) => {
    const path = String(args.path ?? "");
    if (!path.endsWith("git")) throw coded("gitMissing", `git not found: ${path}`, { value: path });
    return "git version 2.47.1";
  },

  connect_git: (args) => {
    return connectGit(args);
  },

  git_status: () => {
    return gitStatus();
  },

  git_stage: (args) => {
    return gitMove(args.paths, true);
  },

  git_unstage: (args) => {
    return gitMove(args.paths, false);
  },

  git_discard: (args) => {
    return gitDiscard(args.path, args.staged);
  },

  git_diff: (args) => {
    return gitDiff(args.path, args.staged);
  },

  git_commit: (args) => {
    gitStatus();
    if (!String(args.message ?? "").trim()) {
      throw coded("gitEmptyMessage", "empty commit message");
    }
    gitState.changes = gitState.changes.filter((change) => {
      change.staged = null;
      return change.unstaged !== null;
    });
    gitState.ahead += 1;
    return gitStatus();
  },
} satisfies Handlers;
