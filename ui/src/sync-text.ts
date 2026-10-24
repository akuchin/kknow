/**
 * Sync texts of the window: the report after a sync and the status bar
 * segment.
 *
 * This module has no imports except types, so the tests run it in Node
 * without Vite. The window passes the translation and the shortcut
 * functions in `SyncWords`, because `i18n.ts` needs Vite.
 */

import type { Action } from "./actions";
import type { SyncReport, SyncStatus } from "./api";
import type { Key, Params } from "./i18n";
import type { SyncTone } from "./statusbar";

/** What the texts need from the window: `t`, the shortcuts and errors. */
export interface SyncWords {
  t: (key: Key, params?: Params) => string;
  shortcut: (action: Action) => string;
  withShortcut: (text: string, action: Action) => string;
  describeError: (error: unknown) => string;
}

/** The sync segment of the status bar. */
export interface SyncSegment {
  text: string;
  tone: SyncTone;
  title: string;
}

export function describeReport(report: SyncReport, { t }: SyncWords): string {
  const counts: [Key, number][] = [
    ["sync.report.uploaded", report.uploaded],
    ["sync.report.downloaded", report.downloaded],
    ["sync.report.removedLocal", report.removedLocal],
    ["sync.report.removedRemote", report.removedRemote],
    ["sync.report.conflicts", report.conflicts.length],
  ];

  const parts = counts
    .filter(([, count]) => count > 0)
    .map(([label, count]) => t("sync.report.entry", { label: t(label), count }));

  return parts.length > 0 ? parts.join(", ") : t("sync.report.upToDate");
}

/** Short sync target: a bucket or a folder name. */
export function shortLocation(location: string | null, { t }: SyncWords): string {
  if (!location) return t("statusbar.sync.off");
  // The folder name for a folder, the address for a bucket: both are recognizable.
  const tail = location.replace(/\/+$/, "").split("/").filter(Boolean).pop();
  return tail ?? location;
}

/**
 * Time since the last successful sync.
 *
 * The window computes it, not the core: "2 min ago" depends on the language,
 * and the time comes as a number.
 */
export function timeAgo(at: number, { t }: SyncWords, now = Date.now()): string {
  const minutes = Math.floor((now - at) / 60000);
  if (minutes < 1) return t("statusbar.sync.justNow");
  if (minutes < 60) return t("statusbar.sync.minutesAgo", { count: minutes });
  return t("statusbar.sync.hoursAgo", { count: Math.floor(minutes / 60) });
}

/**
 * Sync state in the status bar: off, ready, busy, error. `own` is the status
 * of the active vault.
 *
 * Only this function decides what to show. Else, after an error, another
 * handler could switch the segment back to "ready".
 */
export function syncSegment(
  own: SyncStatus | undefined,
  words: SyncWords,
  now = Date.now(),
): SyncSegment {
  const { t, shortcut, withShortcut, describeError } = words;

  if (!own || own.state === "off") {
    // Unconfigured sync is muted: a click opens the form, not a sync.
    return {
      text: t("statusbar.sync.off"),
      tone: "off",
      title: withShortcut(t("sync.title.unset"), "settings.open"),
    };
  }

  const place = shortLocation(own.location, words);
  if (own.state === "syncing") {
    const of = t("statusbar.sync.progress", { done: own.done, total: own.total });
    return {
      text: own.total > 0 ? of : t("statusbar.sync.busy"),
      tone: "busy",
      title: t("status.syncing"),
    };
  }
  if (own.state === "error") {
    return {
      text: t("statusbar.sync.error"),
      tone: "error",
      title: own.message ? describeError(own.message) : "",
    };
  }
  const when = own.at ? ` · ${timeAgo(own.at, words, now)}` : "";
  return {
    text: `${place}${when}`,
    tone: own.state === "conflict" ? "conflict" : "ready",
    title:
      own.state === "conflict"
        ? t("statusbar.sync.conflicts", { count: own.conflicts })
        : t("sync.title", {
            location: own.location ?? place,
            sync: shortcut("vault.sync"),
            settings: shortcut("settings.open"),
          }),
  };
}
