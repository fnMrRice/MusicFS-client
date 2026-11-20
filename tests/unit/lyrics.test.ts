import assert from "node:assert/strict";
import test from "node:test";
import { currentLine, parseLyrics } from "../../src/ui/lyrics.ts";

test("LRC offsets, repeated timestamps and translations share the playback timeline", () => {
  const lines = parseLyrics([
    "[ar:MusicFS]",
    "[offset:+250]",
    "[00:01.5][00:05.050]First",
    "[00:01.500]Translation",
    "[00:03:20]",
  ].join("\r\n"));
  assert.deepEqual(lines, [
    { time: 1250, text: "First\nTranslation" },
    { time: 2950, text: "" },
    { time: 4800, text: "First" },
  ]);
  assert.equal(currentLine(lines, 1249), -1);
  assert.equal(currentLine(lines, 1250), 0);
  assert.equal(currentLine(lines, 4799), 1);
  assert.equal(currentLine(lines, 4801), 2);
});

test("plain lyrics and unsupported timestamps stay readable as plain text", () => {
  const lines = parseLyrics("[00:61]Invalid time\nNo timestamps");
  assert.deepEqual(lines, []);
  assert.equal(currentLine(lines, 0), -1);
});
