import type {
  CommandProblem,
  CommandManifest,
  CommandSpec,
  CommandView,
  RunLine,
  TargetKind,
} from "../api";
import { coded, session } from "./store";
import type { Channel } from "@tauri-apps/api/core";
import type { Handlers } from "./handlers";

export const MOCK_COMMANDS: (CommandSpec & {
  output: { code: number | null; stdout: string; stderr: string };
  lineMs?: number;
})[] = [
  {
    ...blank("confluence", "Send to Confluence"),
    script: 'wiki-publish replace $KKNOW_FM_CONFLUENCE "$KKNOW_PATH"',
    targets: ["note"],
    requires: ["confluence"],
    output: {
      code: 0,
      stdout: "uploading {path}\npage updated: https://wiki.example/pages/12345\n",
      stderr: "",
    },
  },
  {
    ...blank("lint", "Check links"),
    kind: "python",
    script: "import os\nprint(os.environ['KKNOW_PATH'])",
    match: ["Docs/**"],
    output: {
      code: 2,
      stdout: "checked: {target}\n",
      stderr: "two links lead nowhere:\n  [[Ghost]]\n  [[Not written yet]]\n",
    },
  },
  {
    ...blank("backup", "Pack an archive"),
    kind: "fish",
    script: "tar czf ~/backup.tgz $KKNOW_VAULT",
    targets: ["folder", "vault"],
    confirm: true,
    timeoutSeconds: 300,
    lineMs: 700,
    output: {
      code: null,
      stdout:
        "packing the archive…\nJournal/2026-10.md\nJournal/2026-09.md\nProjects/kknow.md\nProjects/plan.md\narchive ready: 4 files\n",
      stderr: "",
    },
  },
  {
    ...blank("broken", "Typo in the kind"),
    kind: "phyton",
    script: "echo",
    output: { code: 0, stdout: "", stderr: "" },
  },
];

export function blank(id: string, name: string): CommandSpec {
  return {
    id,
    name,
    kind: "shell",
    script: "",
    targets: ["note", "folder", "vault"],
    vaults: [],
    match: [],
    requires: [],
    login: false,
    program: null,
    cwd: "vault",
    timeoutSeconds: 60,
    confirm: false,
    env: {},
  };
}

export const MOCK_KINDS = ["shell", "bash", "fish", "zsh", "python"];


export function commandProblems(): CommandProblem[] {
  const problems: CommandProblem[] = [];

  for (const command of MOCK_COMMANDS) {
    const id = command.id;
    if (!id.trim()) {
      problems.push({ code: "noId" });
      continue;
    }
    if (!command.name.trim()) problems.push({ code: "noName", id });
    if (!command.script.trim()) problems.push({ code: "emptyScript", id });
    if (!MOCK_KINDS.includes(command.kind)) {
      problems.push({ code: "unknownKind", id, kind: command.kind, known: MOCK_KINDS.join(", ") });
    }
    if (command.targets.length === 0) problems.push({ code: "noTargets", id });
  }

  return problems;
}

export function healthyCommands(): typeof MOCK_COMMANDS {
  const broken = new Set(commandProblems().map((problem) => ("id" in problem ? problem.id : "")));
  return MOCK_COMMANDS.filter((command) => command.id.trim() && !broken.has(command.id));
}

export function manifestView(): CommandManifest {
  return {
    path: "~/Library/Application Support/kknow/commands.json",
    commands: MOCK_COMMANDS.map(({ output: _output, ...spec }) => spec),
    env: session.sharedEnv,
    problems: commandProblems(),
    kinds: MOCK_KINDS,
    broken: null,
  };
}

export const stoppedRuns = new Set<string>();

export const pause = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export const runsHandlers = {
  list_commands: (args) => {
    const kind = args.kind as TargetKind;
    const file = Boolean(args.file);
    return {
      commands: healthyCommands()
        .filter((command) => command.targets.includes(kind))
        .filter((command) => !file || (command.vaults.length === 0 && command.match.length === 0))
        .map(({ id, name, kind: runner, confirm }) => ({
          id,
          name,
          kind: runner,
          confirm,
        })) satisfies CommandView[],
      problems: commandProblems(),
    };
  },

  run_command: async (args) => {
    const command = MOCK_COMMANDS.find((entry) => entry.id === args.command);
    if (!command) throw coded("unknownCommand", `command not found: ${String(args.command)}`, { value: String(args.command) });

    const run = args.run;
    const lines = args.lines as Channel<RunLine>;
    const stdout = command.output.stdout
      .replace(
        "{path}",
        (args.file) ?? `/Users/kknow/${(args.target) ?? "(vault)"}`,
      )
      .replace("{target}", (args.kind) ?? "vault");
    const printed: RunLine[] = [
      ...stdout.split("\n").filter(Boolean).map((text) => ({ stream: "stdout" as const, text })),
      ...command.output.stderr
        .split("\n")
        .filter(Boolean)
        .map((text) => ({ stream: "stderr" as const, text })),
    ];

    const started = Date.now();
    const sent: RunLine[] = [];
    for (const line of printed) {
      await pause(command.lineMs ?? 60);
      if (stoppedRuns.has(run)) break;
      lines.onmessage(line);
      sent.push(line);
    }
    const stopped = stoppedRuns.delete(run);
    const text = (stream: RunLine["stream"]) =>
      sent
        .filter((line) => line.stream === stream)
        .map((line) => `${line.text}\n`)
        .join("");

    return {
      command: command.id,
      code: stopped ? null : command.output.code,
      stdout: text("stdout"),
      stderr: text("stderr"),
      end: stopped ? "stopped" : command.output.code === null ? "timedOut" : "exited",
      truncated: false,
      millis: Date.now() - started,
    };
  },

  stop_command: (args) => {
    stoppedRuns.add(args.run);
    return;
  },

  edit_commands: () => {
    return manifestView();
  },

  save_commands: (args) => {
    const outputs = new Map(MOCK_COMMANDS.map((command) => [command.id, command.output]));
    const incoming = (args.commands as CommandSpec[]).map((spec) => ({
      ...spec,
      output: outputs.get(spec.id) ?? { code: 0, stdout: "done\n", stderr: "" },
    }));

    MOCK_COMMANDS.splice(0, MOCK_COMMANDS.length, ...incoming);
    session.sharedEnv = args.env as Record<string, string>;
    return manifestView();
  },

  reveal_commands: () => {
    console.info("reveal_commands: the browser has no file manager");
    return;
  },
} satisfies Handlers;
