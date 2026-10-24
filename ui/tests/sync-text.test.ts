// Sync texts of the window (`src/sync-text.ts`). Run: `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import type { SyncReport, SyncStatus } from "../src/api.ts";
import {
  describeReport,
  shortLocation,
  syncSegment,
  timeAgo,
  type SyncWords,
} from "../src/sync-text.ts";

/** Texts as keys with their values: the tests check what the code asks for. */
const words: SyncWords = {
  t: (key, params = {}) => {
    const values = Object.entries(params).map(([name, value]) => `${name}=${value}`);
    return values.length > 0 ? `${key}(${values.join(",")})` : key;
  },
  shortcut: (action) => `<${action}>`,
  withShortcut: (text, action) => `${text} <${action}>`,
  describeError: () => "ошибка",
};

const NOW = 1_800_000_000_000;
const MINUTE = 60_000;

function report(counts: Partial<Omit<SyncReport, "conflicts">>, conflicts = 0): SyncReport {
  return {
    uploaded: 0,
    downloaded: 0,
    removedLocal: 0,
    removedRemote: 0,
    ...counts,
    conflicts: Array.from({ length: conflicts }, (_, index) => ({
      id: `Заметка ${index}`,
      copy: `Заметка ${index} (конфликт)`,
    })) as SyncReport["conflicts"],
  };
}

function status(fields: Partial<SyncStatus>): SyncStatus {
  return {
    vault: "v",
    state: "idle",
    done: 0,
    total: 0,
    at: null,
    message: null,
    conflicts: 0,
    location: "s3://bucket/notes",
    ...fields,
  } as SyncStatus;
}

test("a report lists only the counts that are not zero", () => {
  assert.equal(
    describeReport(report({ uploaded: 2, removedRemote: 1 }, 3), words),
    "sync.report.entry(label=sync.report.uploaded,count=2), " +
      "sync.report.entry(label=sync.report.removedRemote,count=1), " +
      "sync.report.entry(label=sync.report.conflicts,count=3)",
  );
});

test("a report without changes says that all is up to date", () => {
  assert.equal(describeReport(report({}), words), "sync.report.upToDate");
});

test("shortLocation keeps the last segment of a folder or a bucket", () => {
  assert.equal(shortLocation("/Users/me/Dropbox/Заметки/", words), "Заметки");
  assert.equal(shortLocation("s3://bucket", words), "bucket");
  assert.equal(shortLocation("///", words), "///");
  assert.equal(shortLocation(null, words), "statusbar.sync.off");
});

test("timeAgo goes from now to minutes to hours", () => {
  assert.equal(timeAgo(NOW - 30_000, words, NOW), "statusbar.sync.justNow");
  assert.equal(timeAgo(NOW - 5 * MINUTE, words, NOW), "statusbar.sync.minutesAgo(count=5)");
  assert.equal(timeAgo(NOW - 59 * MINUTE, words, NOW), "statusbar.sync.minutesAgo(count=59)");
  assert.equal(timeAgo(NOW - 150 * MINUTE, words, NOW), "statusbar.sync.hoursAgo(count=2)");
});

test("no status or sync off: a muted segment that points to the settings", () => {
  const off = {
    text: "statusbar.sync.off",
    tone: "off",
    title: "sync.title.unset <settings.open>",
  };
  assert.deepEqual(syncSegment(undefined, words, NOW), off);
  assert.deepEqual(syncSegment(status({ state: "off" }), words, NOW), off);
});

test("a running sync shows its progress when the total is known", () => {
  assert.deepEqual(syncSegment(status({ state: "syncing", done: 1, total: 4 }), words, NOW), {
    text: "statusbar.sync.progress(done=1,total=4)",
    tone: "busy",
    title: "status.syncing",
  });
  assert.equal(syncSegment(status({ state: "syncing" }), words, NOW).text, "statusbar.sync.busy");
});

test("an error shows the error text as the title", () => {
  const message = { code: "sync.failed", params: {}, message: "Сбой" };
  assert.deepEqual(syncSegment(status({ state: "error", message }), words, NOW), {
    text: "statusbar.sync.error",
    tone: "error",
    title: "ошибка",
  });
  assert.equal(syncSegment(status({ state: "error" }), words, NOW).title, "");
});

test("a ready vault shows the target and the time of the last sync", () => {
  assert.deepEqual(syncSegment(status({ at: NOW - 3 * MINUTE }), words, NOW), {
    text: "notes · statusbar.sync.minutesAgo(count=3)",
    tone: "ready",
    title: "sync.title(location=s3://bucket/notes,sync=<vault.sync>,settings=<settings.open>)",
  });
  assert.equal(syncSegment(status({}), words, NOW).text, "notes");
});

test("conflicts change the tone and the title", () => {
  assert.deepEqual(syncSegment(status({ state: "conflict", conflicts: 2 }), words, NOW), {
    text: "notes",
    tone: "conflict",
    title: "statusbar.sync.conflicts(count=2)",
  });
});
