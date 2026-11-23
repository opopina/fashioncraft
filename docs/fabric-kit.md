# Fabric kit: measure a real fabric

This procedure turns a sample of a real fabric into a FashionCraft fabric with tools any workshop has: a kitchen or jewellery scale, a ruler or caliper, binder clips, weights and a piece of card cut at 41.5°. Type the readings in **Materials › Measure fabric…**; FashionCraft runs the same tests on a virtual copy of the strips and searches for the fabric that gives your readings. It then keeps the fabric in your library with the measurements, and cuts the selected piece from it.

The workflow follows the fabric kits that desktop garment tools describe publicly (weigh, stretch with weights, bend over an edge, type the readings). The individual tests are classic textile tests simplified for a workshop: the strip method of ISO 13934-1, the extensibility loads of the FAST system (CSIRO), the cantilever of ASTM D1388 (option A), and an incline test for friction. None of them is done here to a standard's full rigour; the point is a fabric that drapes like yours.

How the fit works, and its limits, are in [fabric-kit-fit.md](fabric-kit-fit.md).

## What you need

| Tool | For | Good enough |
|---|---|---|
| Scale | Weight per m² | 0.01 g resolution (a jewellery scale); 0.1 g with a larger square |
| Rotary cutter, mat, metal ruler, 10 × 10 cm template | Cutting squares and strips square to the grain | |
| Caliper (digital) or thickness gauge | Thickness | 0.01 mm resolution |
| Steel ruler | Strip lengths, overhangs | 0.5 mm divisions; a caliper reads 0.1 mm |
| 4 binder clips (50 mm) and 4 flat sticks 60 mm long (wood or aluminium) | Two clamps for the strips | |
| A hook or a nail, a bag or bucket, known weights (25 g to 2 kg) | Hanging loads | Kitchen weights, coins, water bottles weighed on the scale |
| A table with a straight, square edge, a card triangle cut at 41.5° | Cantilever | A protractor to draw the triangle |
| A board, a small wooden block, a phone with an inclinometer app | Friction (optional) | A protractor |
| Fine pen or tailor's chalk | Marks | |

## Before you start

- Leave the fabric flat for a day in the room where you test (fabrics change with humidity), away from radiators and draughts.
- Find the **warp**: it runs along the selvedge. Mark it with an arrow on every specimen you cut.
- Cut away from the selvedge (at least 10 cm in) and from folds and creases. Do not iron the specimens; handle them by the ends.
- Cut with a rotary cutter against a metal ruler. A strip cut off-grain behaves like a mix of warp and bias.

## 1. Weight per m²

1. Cut three 10 × 10 cm squares from different places of the fabric.
2. Weigh each. Its grams × 100 are its g/m² (a 20 × 20 cm square: grams × 25).
3. Type the mean as **Area weight**.

## 2. Thickness

1. Measure at five places with the caliper, closing it gently until the jaws just touch the fabric without squeezing it; or stack ten layers, measure the stack and divide by ten.
2. Type the mean as **Thickness**. It sets the fabric's thickness in the simulation; it does not change how it stretches or bends.

## 3. Strip test with weights (warp, weft, bias)

**Specimens.** Three strips, each 50 mm wide and 300 mm long: one cut along the warp, one along the weft and one on the **bias** (at 45° to both). Strips for knits may curl at the edges: cut them a little wider and trim the curl.

**Marks.** Lay a strip flat and relaxed on the table. Draw two thin lines across it, **100 mm apart**, centred on the strip.

**Clamps.** A clamp is two flat sticks either side of the strip, held by two binder clips. Grip the strip across its full width with the clamp's edge square to the strip. Leave **200 mm of free strip between the two clamps**, so each mark is 50 mm from its clamp. Hang the top clamp from a hook or nail; hang a hook or a bag from the bottom clamp.

**Weigh the bottom clamp** with its hook or bag: it is part of every load. Each **load** you type is everything hanging from the strip: bottom clamp, hook or bag, and the weights in it.

**Loads.** Choose three loads, smallest first. As a guide for a 50 mm strip:

| Fabric | Warp and weft | Bias |
|---|---|---|
| Knits, stretch fabrics | 25, 100, 500 g | 25, 100 g |
| Light and medium wovens | 500, 1000, 2000 g | 25, 100 g |
| Heavy wovens, denim, canvas | 1, 2, 4 kg | 50, 200 g |

25, 100 and 500 g on 5 cm are FAST's 5, 20 and 100 gf/cm. A useful rule: the largest load should stretch the gauge by **at least 5 mm**; less than that and one ruler step is a large share of the reading (the window warns you). The bias strip shears easily: use small loads, or the fabric locks and stops behaving like a simple sheet.

**Readings.**

1. Hang the largest load for ten seconds and take it off, twice: a fresh strip settles on the first pulls.
2. Hang the smallest load. Wait **one minute**: fabrics keep stretching for a while.
3. Read the distance between the two marks, at the **centre of the strip**, to 0.5 mm with the ruler (0.1 mm with a caliper). Type load and length.
4. Add the next load without taking the first off, wait a minute, read. Then the largest.
5. Do the same for the weft strip and the bias strip.

If a strip slips in a clamp, or still stretches after a minute, start again with a fresh strip and grip it harder. If it does not come back near its 100 mm when you unload it, the largest load was too much: use smaller ones.

**In the window:** Strip width 50 mm, Length between clamps 200 mm, Distance between marks 100 mm, Ruler resolution 0.5 mm (or 0.1 with a caliper), then the readings in the Warp, Weft and Bias tabs. Other dimensions work too: type the ones you used.

## 4. Cantilever at 41.5° (warp, weft)

**The bench.** A table with a straight, square edge, its top level. Cut a right triangle from stiff card with one angle of **41.5°**. Stand it against the side of the table so that its long edge runs from the table's top edge down and away at 41.5° below the horizontal. You will watch the strip's tip from the side, against that edge.

**Specimens.** Four strips 25 mm wide and 200 mm long: two along the warp, two along the weft.

**Readings.**

1. Lay a strip on the table, its end flush with the table edge, and lay the steel ruler on top of it, the ruler's zero at the strip's end.
2. Push ruler and strip together, slowly and steadily, over the edge. The strip bends down under its own weight.
3. Stop when the strip's tip just touches the 41.5° line, seen from the side at eye level. Read the **overhang**: the length of strip beyond the table edge.
4. Read each strip four times: from both ends, face up and face down. Type the **mean of the four readings** of the warp strips as **Warp overhang** and of the weft strips as **Weft overhang**.

Strips that curl or twist give wrong readings: flatten them by hand before, or cut new ones. Very limp fabrics (a long, irregular overhang) and very stiff ones (more than 200 mm) are outside what this test reads well.

The standard's bending length is half the overhang, and its flexural rigidity G = w·g·c³ (w the weight per m², c the bending length). The window does not need either: it fits the overhang itself.

## 5. Friction (optional)

1. Tape a piece of the fabric, face up, flat on a board.
2. Wrap a small wooden block (about 5 × 5 cm, 100 g or more) in the same fabric, face out, and lay it on the board.
3. Tilt the board slowly from flat until the block starts to slide. Read the angle with a phone inclinometer or a protractor.
4. Repeat three times and type the mean as **Slide angle**. The friction coefficient is μ = tan θ.

FashionCraft uses one friction coefficient for every contact in a scene: **Use friction in simulation** puts this one there.

## 6. In FashionCraft

1. **Materials › Measure fabric…** Type the name, the weight and thickness, the strip readings (tabs Warp, Weft, Bias), the overhangs and, if you have it, the slide angle. **Example readings** fills in a light cotton poplin to see how it works.
2. **Fit fabric.** The fit runs in the background; a few seconds to a minute.
3. Read the table **Readings and virtual tests**: each of your readings beside the one the fitted fabric gives. The notes underneath say what was missing, what was estimated and which readings the model could not follow.
4. **Save to library** keeps it in the library's `Fabrics` folder as a `.fcfabric` file with your measurements. The library's **Fabrics** tab lists them: **Open** shows the measurements and fit again, **Apply to piece** cuts the selected piece from it.
5. **Apply to selected piece** cuts the selected piece from the fitted fabric now. Simulate to see it drape.

Only the warp or the weft strip is required. Without a weft strip the fabric stretches the same both ways; without the bias strip its shear is estimated; without a cantilever it keeps the default bending. Each of these is said in the notes.
