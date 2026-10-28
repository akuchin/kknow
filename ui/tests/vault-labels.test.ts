// The label filter of the sidebar (`src/vault-labels.ts`). Run: `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { filterVaults, labelsOf, livePicks } from "../src/vault-labels.ts";

const vaults = [
  { id: "notes", labels: ["work", "ai"] },
  { id: "work", labels: ["work"] },
  { id: "home", labels: ["личное"] },
  { id: "archive", labels: [] },
];

const ids = (list: { id: string }[]): string[] => list.map((vault) => vault.id);

test("labelsOf lists each label once, in the order of the vaults", () => {
  assert.deepEqual(labelsOf(vaults), ["work", "ai", "личное"]);
  assert.deepEqual(labelsOf([]), []);
});

test("no picked label shows every vault", () => {
  assert.deepEqual(ids(filterVaults(vaults, [], null)), ["notes", "work", "home", "archive"]);
});

test("picked labels leave the vaults with any of them", () => {
  assert.deepEqual(ids(filterVaults(vaults, ["work"], null)), ["notes", "work"]);
  assert.deepEqual(ids(filterVaults(vaults, ["ai", "личное"], null)), ["notes", "home"]);
});

test("the active vault stays without the label", () => {
  assert.deepEqual(ids(filterVaults(vaults, ["личное"], "archive")), ["home", "archive"]);
});

test("livePicks forgets labels that no vault has", () => {
  assert.deepEqual(livePicks(["work", "старое"], vaults), ["work"]);
});
