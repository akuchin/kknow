// Tests for pasted image names (`src/attachment-name.ts`). Run: `pnpm ui:test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { imageLink, pastedImageName } from "../src/attachment-name.ts";

test("a pasted image gets a name with the local time", () => {
  const now = new Date(2026, 9, 4, 5, 3, 9);

  assert.equal(pastedImageName("image/png", now), "Pasted image 20261004050309.png");
  assert.equal(pastedImageName("image/JPEG", now), "Pasted image 20261004050309.jpg");
  assert.equal(pastedImageName("image/svg+xml", now), "Pasted image 20261004050309.svg");
});

test("a type the loader cannot show gets no name", () => {
  const now = new Date(2026, 9, 24);

  assert.equal(pastedImageName("image/tiff", now), null);
  assert.equal(pastedImageName("text/plain", now), null);
  assert.equal(pastedImageName("", now), null);
});

test("the link is an embed with the path as is", () => {
  assert.equal(
    imageLink("attachments/Pasted image 20261024153012.png"),
    "![[attachments/Pasted image 20261024153012.png]]",
  );
});
