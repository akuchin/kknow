/**
 * Texts for the data that the core sends without a ready text.
 *
 * The core gives a `code` and its values, and each language has its own
 * words for them: the problems of the hand-edited files, the events of a
 * thread. The key comes from the type of the code, so a code without a
 * translation is a type error, not a raw key on the screen.
 *
 * The names that kknow ships (the built-in themes, the default commands) are
 * here too: they are data, but the person did not write them.
 */

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

/** A problem of a theme file, or a variable that a theme lost. */
export function themeProblemText(problem: ThemeProblem | ThemeWarning): string {
  const { code, ...values } = problem;
  return t(`problem.theme.${code}`, values);
}

/** A skipped entry of the keys file. */
export function keyProblemText(problem: KeyProblem): string {
  const { code, ...values } = problem;
  return t(`problem.key.${code}`, values);
}

/** A skipped entry of the commands file. */
export function commandProblemText(problem: CommandProblem): string {
  const { code, ...values } = problem;
  return t(`problem.command.${code}`, values);
}

/** The command that a problem is about, if the entry has an ID. */
export function problemCommand(problem: CommandProblem): string | null {
  return "id" in problem ? problem.id : null;
}

/** The name of a theme. A built-in theme has a name in each language. */
export function themeName(theme: Pick<Theme, "id" | "name" | "source">): string {
  return (theme.source === "builtIn" && maybe(`theme.builtIn.${theme.id}`)) || theme.name;
}

/**
 * The name of a command.
 *
 * kknow writes its default commands into the file with English names. While
 * the person keeps such a name, the window shows it in its own language. A
 * name that the person changed is theirs and stays as it is.
 */
export function commandName(command: { id: string; name: string }): string {
  const key = `command.shipped.${command.id}`;
  if (textIn("en", key) !== command.name) return command.name;
  return maybe(key) ?? command.name;
}

/**
 * The author of a thread message. Old threads have "агент" for an agent
 * that did not give its name.
 */
export function messageAuthor(message: Pick<ThreadMessage, "from">): string {
  if (message.from === "me") return t("thread.me");
  if (message.from === "agent" || message.from === "агент") return t("thread.agent");
  return message.from;
}

/** A message that kknow wrote into a thread. */
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

/** The text of a thread message in this language. */
export function messageText(message: Pick<ThreadMessage, "text" | "event">): string {
  return message.event ? threadEventText(message.event) : message.text;
}
