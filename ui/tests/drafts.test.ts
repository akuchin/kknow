// Checks the store of unsaved field values (`src/drafts.ts`). Run: `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import {
  clearDraft,
  clearDraftsUnder,
  draftChecked,
  draftValue,
  hasDraft,
  setDraft,
  setDraftChecked,
} from "../src/drafts.ts";

test("a draft outlives the field and wins over the saved value", () => {
  setDraft("reply:n:a", "typed");
  assert.equal(draftValue("reply:n:a", "saved"), "typed");
  assert.equal(draftValue("reply:n:b", "saved"), "saved");
  clearDraft("reply:n:a");
});

test("a value equal to the saved one is not a draft", () => {
  setDraft("sync:v:s3:endpoint", "https://a", "https://a");
  assert.equal(hasDraft("sync:v:s3:endpoint"), false);
  setDraft("sync:v:s3:endpoint", "https://b", "https://a");
  setDraft("sync:v:s3:endpoint", "https://a", "https://a");
  assert.equal(hasDraft("sync:v:s3:endpoint"), false);
});

test("clearing one reply keeps the reply to another thread", () => {
  setDraft("reply:n:t1", "first");
  setDraft("reply:n:t1:more", "a note whose text starts the same");
  clearDraft("reply:n:t1");
  assert.equal(hasDraft("reply:n:t1"), false);
  assert.equal(draftValue("reply:n:t1:more"), "a note whose text starts the same");
  clearDraft("reply:n:t1:more");
});

test("a saved form clears all of its fields and nothing else", () => {
  setDraft("sync:v:s3:bucket", "notes");
  setDraft("sync:v:s3:secret", "key");
  setDraft("sync:w:s3:bucket", "other vault");
  setDraftChecked("sync:v:s3:flag", false, true);
  clearDraftsUnder("sync:v:s3:");
  assert.equal(hasDraft("sync:v:s3:bucket"), false);
  assert.equal(hasDraft("sync:v:s3:secret"), false);
  assert.equal(hasDraft("sync:v:s3:flag"), false);
  assert.equal(draftValue("sync:w:s3:bucket"), "other vault");
  clearDraftsUnder("sync:w:");
});

test("a checkbox draft remembers an unchecked box", () => {
  setDraftChecked("proposal:p:edit:1", false, true);
  assert.equal(draftChecked("proposal:p:edit:1", true), false);
  setDraftChecked("proposal:p:edit:1", true, true);
  assert.equal(hasDraft("proposal:p:edit:1"), false);
});
