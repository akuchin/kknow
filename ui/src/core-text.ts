import type {
  CommandProblem,
  KeyProblem,
  Theme,
  ThemeProblem,
  ThemeWarning,
  ThreadEvent,
  ThreadMessage,
} from "./bindings";
import { maybe, t, textIn } from "./i18n";

export function themeProblemText(problem: ThemeProblem | ThemeWarning): string {
  const { code, ...values } = problem;
  return t(`problem.theme.${code}`, values);
}

export function keyProblemText(problem: KeyProblem): string {
  const { code, ...values } = problem;
  return t(`problem.key.${code}`, values);
}

export function commandProblemText(problem: CommandProblem): string {
  const { code, ...values } = problem;
  return t(`problem.command.${code}`, values);
}

export function problemCommand(problem: CommandProblem): string | null {
  return "id" in problem ? problem.id : null;
}

export function themeName(theme: Pick<Theme, "id" | "name" | "source">): string {
  return (theme.source === "builtIn" && maybe(`theme.builtIn.${theme.id}`)) || theme.name;
}

export function commandName(command: { id: string; name: string }): string {
  const key = `command.shipped.${command.id}`;
  if (textIn("en", key) !== command.name) return command.name;
  return maybe(key) ?? command.name;
}

export function messageAuthor(message: Pick<ThreadMessage, "from">): string {
  if (message.from === "me") return t("thread.me");
  if (message.from === "agent" || message.from === "агент") return t("thread.agent");
  return message.from;
}

export function threadEventText(event: ThreadEvent): string {
  switch (event.code) {
    case "proposed":
      return t("thread.event.proposed", { proposal: event.proposal, reason: event.reason });
    case "rejected":
      return event.feedback
        ? t("thread.event.rejected.feedback", { proposal: event.proposal, feedback: event.feedback })
        : t("thread.event.rejected", { proposal: event.proposal });
    case "accepted": {
      const params = { proposal: event.proposal, feedback: event.feedback };
      if (event.partly) {
        return event.feedback
          ? t("thread.event.partly.feedback", params)
          : t("thread.event.partly", params);
      }
      return event.feedback
        ? t("thread.event.accepted.feedback", params)
        : t("thread.event.accepted", params);
    }
  }
}

export function messageText(message: Pick<ThreadMessage, "text" | "event">): string {
  return message.event ? threadEventText(message.event) : message.text;
}
