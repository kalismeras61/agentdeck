/**
 * Lays out a commit graph: which column ("lane") each commit sits in and the line pieces that
 * connect it to its parents. Commits come newest first in topological order (git log
 * --topo-order), which keeps each branch's commits together.
 */

export interface GraphCommit {
  hash: string;
  parents: string[];
}

/** A line piece inside one row: from a lane at the top/middle to a lane at the middle/bottom. */
export interface Segment {
  from: number;
  to: number;
  /** 0 = top edge, 1 = the commit's dot, 2 = bottom edge. */
  y1: 0 | 1;
  y2: 1 | 2;
  color: number;
}

export interface GraphRow {
  lane: number;
  color: number;
  /** Lanes in use in this row, for its width. */
  width: number;
  segments: Segment[];
}

export function layoutGraph(commits: GraphCommit[]): GraphRow[] {
  const lanes: (string | null)[] = [];
  const colors: number[] = [];
  let nextColor = 0;
  const rows: GraphRow[] = [];

  const freeSlot = () => {
    const i = lanes.indexOf(null);
    return i === -1 ? lanes.length : i;
  };

  for (const c of commits) {
    let lane = lanes.indexOf(c.hash);
    if (lane === -1) {
      lane = freeSlot();
      lanes[lane] = c.hash;
      colors[lane] = nextColor++;
    }
    const color = colors[lane]!;
    const before = lanes.slice();
    const segments: Segment[] = [];

    // Lines coming in from above.
    before.forEach((h, j) => {
      if (h === null) return;
      if (h === c.hash) {
        if (j === lane ? rows.length > 0 && wasOpen(rows, j) : true) segments.push({ from: j, to: lane, y1: 0, y2: 1, color: colors[j]! });
      } else {
        segments.push({ from: j, to: j, y1: 0, y2: 2, color: colors[j]! });
      }
    });

    // Lanes that were waiting for this commit end here.
    before.forEach((h, j) => {
      if (h === c.hash && j !== lane) lanes[j] = null;
    });

    // The first parent continues this lane; the others join or open lanes.
    const [first, ...rest] = c.parents;
    if (first !== undefined) {
      const elsewhere = lanes.findIndex((h, j) => h === first && j !== lane);
      if (elsewhere !== -1 && elsewhere < lane) {
        // Already expected in a lane to the left (a fork point): run into that lane.
        lanes[lane] = null;
        segments.push({ from: lane, to: elsewhere, y1: 1, y2: 2, color: colors[elsewhere]! });
      } else if (elsewhere !== -1) {
        // Expected in a lane to the right: this lane carries on and that one bends into it,
        // so the main line stays on the left.
        lanes[lane] = first;
        lanes[elsewhere] = null;
        const through = segments.find((x) => x.from === elsewhere && x.to === elsewhere && x.y1 === 0 && x.y2 === 2);
        if (through) through.to = lane;
        segments.push({ from: lane, to: lane, y1: 1, y2: 2, color });
      } else {
        lanes[lane] = first;
        segments.push({ from: lane, to: lane, y1: 1, y2: 2, color });
      }
    } else {
      lanes[lane] = null;
    }
    for (const p of rest) {
      let k = lanes.indexOf(p);
      if (k === -1) {
        k = freeSlot();
        lanes[k] = p;
        colors[k] = nextColor++;
      }
      segments.push({ from: lane, to: k, y1: 1, y2: 2, color: colors[k]! });
    }

    while (lanes.length && lanes[lanes.length - 1] === null) lanes.pop();
    rows.push({ lane, color, width: Math.max(before.length, lanes.length, lane + 1), segments });
  }
  return rows;
}

/** Whether the previous row left a line running down this lane. */
function wasOpen(rows: GraphRow[], lane: number): boolean {
  const prev = rows[rows.length - 1];
  return Boolean(prev?.segments.some((s) => s.y2 === 2 && s.to === lane));
}
