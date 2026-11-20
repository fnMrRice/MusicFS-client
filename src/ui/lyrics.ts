export interface TimedLine {
  time: number;
  text: string;
}

export function parseLyrics(raw: string) {
  const offset = Number(raw.match(/^\s*\[offset:\s*([+-]?\d+)\s*\]\s*$/im)?.[1] ?? 0);
  const lines: TimedLine[] = [];
  for (const line of raw.split(/\r?\n/)) {
    const tags = [...line.matchAll(/\[(\d{1,3}):([0-5]\d)(?:[.:](\d{1,3}))?\]/g)];
    const last = tags.at(-1);
    if (!last) continue;
    const text = line.slice(last.index! + last[0].length).trim();
    for (const tag of tags) {
      const time = (Number(tag[1]) * 60 + Number(tag[2])) * 1000 + Number((tag[3] ?? "").padEnd(3, "0"));
      lines.push({ time: Math.max(0, time - offset), text });
    }
  }
  lines.sort((a, b) => a.time - b.time);
  const grouped: TimedLine[] = [];
  for (const line of lines) {
    const previous = grouped.at(-1);
    if (previous?.time === line.time) {
      if (line.text) previous.text = [previous.text, line.text].filter(Boolean).join("\n");
    } else grouped.push(line);
  }
  return grouped;
}

export function currentLine(lines: TimedLine[], position: number) {
  let low = 0;
  let high = lines.length;
  while (low < high) {
    const mid = Math.floor((low + high) / 2);
    if (lines[mid].time <= position) low = mid + 1;
    else high = mid;
  }
  return low - 1;
}
