"""Print the call tree of macOS `sample` output, merged by function, to a depth.

Usage: python3 -I bench/tree.py sample.txt <root-substring> [depth] [min]
Profile a running cloth-fit with `sample <pid> 6 -file sample.txt` (macOS).
Merges all occurrences of the root (any thread) and shows children by total samples.
"""
import re
import sys
from collections import defaultdict

path, root = sys.argv[1], sys.argv[2]
depth = int(sys.argv[3]) if len(sys.argv) > 3 else 3
minimum = int(sys.argv[4]) if len(sys.argv) > 4 else 5
line_re = re.compile(r"^([ +!:|]*)(\d+) (.*?)  \(in ")
nodes = []  # (indent, count, name)
started = False
for line in open(path, errors="replace"):
    if line.startswith("Call graph:"):
        started = True
        continue
    if started and line.startswith("Total number"):
        break
    m = line_re.match(line)
    if not m:
        continue
    name = m.group(3)
    name = re.sub(r"^_R[A-Za-z]*?(fashioncraft|wgpu_core|wgpu_hal|rayon_core|wgpu)", r"\1", name)
    name = re.sub(r"Cs[0-9A-Za-z_]{8,13}_", "", name)
    nodes.append((len(m.group(1)), int(m.group(2)), name[:110]))

tree = lambda: defaultdict(lambda: [0, tree()])
agg = tree()
for i, (ind, count, name) in enumerate(nodes):
    if root not in name:
        continue
    # skip nested occurrences of root inside another root occurrence
    stack = [(ind, agg)]
    agg_root = agg[name]
    agg_root[0] += count
    path_stack = [(ind, agg_root)]
    for j in range(i + 1, len(nodes)):
        ind2, c2, n2 = nodes[j]
        if ind2 <= ind:
            break
        while path_stack and path_stack[-1][0] >= ind2:
            path_stack.pop()
        if not path_stack or len(path_stack) > depth:
            continue
        node = path_stack[-1][1][1][n2]
        node[0] += c2
        path_stack.append((ind2, node))


def show(d, level):
    for name, (count, children) in sorted(d.items(), key=lambda kv: -kv[1][0]):
        if count < minimum:
            continue
        print("  " * level + f"{count:6d} {name}")
        show(children, level + 1)


show(agg, 0)
