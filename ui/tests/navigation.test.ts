// Back and forward (`src/navigation.ts`). Run: `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { History, type Place } from "../src/navigation.ts";

const note = (id: string): Place => ({ file: false, vault: "v", id });

/** A window that can open everything except the `missing` ids. */
function walker(missing: string[] = []) {
  let here: Place | null = null;
  return {
    open: async (place: Place) => {
      if (place.file || !missing.includes(place.id)) here = place;
    },
    here: () => here,
    set: (place: Place) => (here = place),
  };
}

test("back and forward walk the opened notes", async () => {
  const history = new History();
  const window = walker();
  for (const id of ["a", "b", "c"]) {
    window.set(note(id));
    history.remember(note(id));
  }

  await history.travel(-1, window.open, window.here);
  assert.deepEqual(window.here(), note("b"));
  assert.ok(history.canBack && history.canForward);

  await history.travel(1, window.open, window.here);
  assert.deepEqual(window.here(), note("c"));
  assert.ok(!history.canForward);
});

test("an open during travel is not a new step", async () => {
  const history = new History();
  history.remember(note("a"));
  history.remember(note("b"));

  await history.travel(-1, async (place) => void history.remember(place), () => note("a"));

  assert.ok(history.canForward, "the step back did not erase the step forward");
});

test("a new step after back drops the forward entries", async () => {
  const history = new History();
  const window = walker();
  history.remember(note("a"));
  history.remember(note("b"));
  await history.travel(-1, window.open, window.here);

  history.remember(note("c"));

  assert.ok(!history.canForward);
});

test("a deleted note leaves the history", async () => {
  const history = new History();
  const window = walker(["b"]);
  for (const id of ["a", "b", "c"]) history.remember(note(id));
  window.set(note("c"));

  await history.travel(-1, window.open, window.here);
  assert.deepEqual(window.here(), note("c"), "the deleted note did not open");

  await history.travel(-1, window.open, window.here);
  assert.deepEqual(window.here(), note("a"), "the next step goes past it");
});

test("the history keeps at most `limit` places", () => {
  const history = new History(2);
  for (const id of ["a", "b", "c"]) history.remember(note(id));
  assert.ok(history.canBack);
});
