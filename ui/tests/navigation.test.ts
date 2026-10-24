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
  for (const id of ["а", "б", "в"]) {
    window.set(note(id));
    history.remember(note(id));
  }

  await history.travel(-1, window.open, window.here);
  assert.deepEqual(window.here(), note("б"));
  assert.ok(history.canBack && history.canForward);

  await history.travel(1, window.open, window.here);
  assert.deepEqual(window.here(), note("в"));
  assert.ok(!history.canForward);
});

test("an open during travel is not a new step", async () => {
  const history = new History();
  history.remember(note("а"));
  history.remember(note("б"));

  await history.travel(-1, async (place) => void history.remember(place), () => note("а"));

  assert.ok(history.canForward, "шаг назад не стёр шаг вперёд");
});

test("a new step after back drops the forward entries", async () => {
  const history = new History();
  const window = walker();
  history.remember(note("а"));
  history.remember(note("б"));
  await history.travel(-1, window.open, window.here);

  history.remember(note("в"));

  assert.ok(!history.canForward);
});

test("a deleted note leaves the history", async () => {
  const history = new History();
  const window = walker(["б"]);
  for (const id of ["а", "б", "в"]) history.remember(note(id));
  window.set(note("в"));

  await history.travel(-1, window.open, window.here);
  assert.deepEqual(window.here(), note("в"), "удалённая не открылась");

  await history.travel(-1, window.open, window.here);
  assert.deepEqual(window.here(), note("а"), "следующий шаг идёт мимо неё");
});

test("the history keeps at most `limit` places", () => {
  const history = new History(2);
  for (const id of ["а", "б", "в"]) history.remember(note(id));
  assert.ok(history.canBack);
});
