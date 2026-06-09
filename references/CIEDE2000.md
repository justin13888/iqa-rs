# CIEDE2000 Color-Difference Formula — Formal Implementation Reference

A complete, unambiguous specification of the mathematics required to implement
the CIEDE2000 ($\Delta E_{00}$) color-difference metric. Every branch, sign
convention, and edge case is stated explicitly. Cross-referenced against the
sources listed below; where the primary standard is ambiguous, the resolution
adopted by Sharma–Wu–Dalal [3] is used and flagged.

## Sources

1. **CIE 142-2001** — *Improvement to Industrial Colour-Difference Evaluation*, CIE Publication No. 142, Central Bureau of the CIE, Vienna (2001). The defining standard. [S1]
2. **Luo, M. R., Cui, G., Rigg, B. (2001)** — "The development of the CIE 2000 colour-difference formula: CIEDE2000," *Color Research & Application* **26**(4), 340–350. Original development paper; equations given in its appendix. [S2]
3. **Sharma, G., Wu, W., Dalal, E. N. (2005)** — "The CIEDE2000 color-difference formula: Implementation notes, supplementary test data, and mathematical observations," *Color Research & Application* **30**(1), 21–30. Canonical implementation reference; supplies the 34-pair test set and resolves the mean-hue / hue-difference ambiguities. [S3]

These three agree on all constants and equation forms; the constants below were
additionally confirmed against multiple independent secondary implementations.
The CIEDE2000 difference is **symmetric** in its two arguments: subscript 1
denotes the reference, subscript 2 the sample [S3].

> **Note on discontinuities.** $\Delta E_{00}$ is *not* a globally continuous
> function of its CIELAB inputs. Discontinuities arise only for colors ~180°
> apart in hue (near-neutral pairs); for samples within 5 CIELAB units the
> discontinuity magnitude is bounded by **0.2734**, and within 1 unit by
> **0.0119** [S3]. This matters for Taylor-series / gradient-based use, not for
> ordinary difference evaluation.

---

## 0. Inputs, parameters, and conventions

**Inputs.** Two CIELAB triples $(L_1^*, a_1^*, b_1^*)$ and $(L_2^*, a_2^*, b_2^*)$,
computed under the **same white point** (CIEDE2000 is defined on CIELAB and does
not itself specify the illuminant).

**Parametric factors.** $k_L,\, k_C,\, k_H$ account for experimental viewing
conditions. Under the CIE reference conditions they are unity:
$$k_L = k_C = k_H = 1.$$
Use these defaults unless a specific application dictates otherwise [S1][S3].

**Angle convention — critical.** All hue angles are in **degrees**, normalized to
$[0°, 360°)$. **Every** trigonometric function below takes its argument in
degrees (equivalently, convert to radians before calling library `sin`/`cos`).
The literal constants $30, 6, 63, 275, 25$ inside the trig terms are degrees.

Throughout, $\overline{\,\cdot\,}$ denotes an arithmetic mean of the two samples,
a prime ($'$) denotes the chroma-adjusted quantities of Step 1, and `atan2(y, x)`
is the four-quadrant arctangent.

---

## 1. Adjusted chroma $C_i'$ and adjusted hue $h_i'$

**1.1 — CIELAB chroma of each sample:**
$$C_{i,ab}^* = \sqrt{a_i^{*2} + b_i^{*2}}, \qquad i = 1, 2.$$

**1.2 — Mean of the two CIELAB chromas:**
$$\overline{C_{ab}^*} = \frac{C_{1,ab}^* + C_{2,ab}^*}{2}.$$

**1.3 — The $a^*$-axis scaling factor $G$** (improves near-neutral / gray performance) [S2]:
$$G = 0.5\left(1 - \sqrt{\dfrac{\overline{C_{ab}^*}^{\,7}}{\overline{C_{ab}^*}^{\,7} + 25^{7}}}\right).$$

**1.4 — Adjusted Cartesian coordinates** ($a^*$ is rescaled; $b^*$ and $L^*$ are unchanged):
$$a_i' = (1 + G)\,a_i^*, \qquad b_i' = b_i^*, \qquad i = 1, 2.$$

**1.5 — Adjusted chroma:**
$$C_i' = \sqrt{a_i'^{\,2} + b_i'^{\,2}}, \qquad i = 1, 2.$$

**1.6 — Adjusted hue angle** (four-quadrant; result mapped into $[0°,360°)$):
$$
h_i' =
\begin{cases}
0, & \text{if } a_i' = 0 \text{ and } b_i' = 0,\\[4pt]
\big(\operatorname{atan2}(b_i', a_i')\big) \bmod 360°, & \text{otherwise.}
\end{cases}
$$

> **Pitfall (hue indeterminacy).** When $a_i' = b_i' = 0$ the hue is undefined; it
> is **defined to be 0** [S3]. `atan2(0,0)` is implementation-defined, so test
> this branch explicitly.
>
> **Pitfall (range).** Library `atan2` returns $(-\pi, \pi]$. Convert to degrees
> and add $360°$ to any negative result so that $h_i' \in [0°, 360°)$. The $[0,360)$
> normalization is mandatory because the $\Delta\theta$ term (Step 4) is **not**
> invariant to $360°$ shifts [S3].

---

## 2. Differences $\Delta L'$, $\Delta C'$, $\Delta h'$, $\Delta H'$

All three primed differences are **signed** (sample 2 minus reference 1). Using
absolute values here is a common and hard-to-detect error [S3].

**2.1 — Lightness difference:**
$$\Delta L' = L_2^* - L_1^*.$$

**2.2 — Chroma difference:**
$$\Delta C' = C_2' - C_1'.$$

**2.3 — Hue-angle difference** $\Delta h'$ (degrees). With $d = h_2' - h_1'$:
$$
\Delta h' =
\begin{cases}
0, & \text{if } C_1' C_2' = 0,\\[4pt]
d, & \text{if } C_1' C_2' \neq 0 \text{ and } |d| \le 180°,\\[4pt]
d - 360°, & \text{if } C_1' C_2' \neq 0 \text{ and } d > 180°,\\[4pt]
d + 360°, & \text{if } C_1' C_2' \neq 0 \text{ and } d < -180°.
\end{cases}
$$

> **Pitfall (sign of $\Delta h'$).** The branch selection uses the **signed**
> quantity $d = h_2' - h_1'$ for the $\pm 360°$ tests, but the **magnitude**
> $|d|$ for the "$\le 180°$" test. Getting this wrong flips the sign of
> $\Delta H'$ and corrupts the rotation term $R_T$ (Step 5) in the blue region.

**2.4 — Hue difference $\Delta H'$** (note the geometric-mean chroma weighting):
$$\Delta H' = 2\sqrt{C_1' C_2'}\;\sin\!\left(\frac{\Delta h'}{2}\right).$$

---

## 3. Mean values $\overline{L'}$, $\overline{C'}$, $\overline{h'}$

**3.1 — Mean lightness and mean chroma** (plain arithmetic means):
$$\overline{L'} = \frac{L_1^* + L_2^*}{2}, \qquad \overline{C'} = \frac{C_1' + C_2'}{2}.$$

**3.2 — Mean hue angle $\overline{h'}$** (degrees). With $s = h_1' + h_2'$ and
$\delta = |h_1' - h_2'|$, evaluate the cases **in order**:
$$
\overline{h'} =
\begin{cases}
s, & \text{if } C_1' C_2' = 0,\\[4pt]
\dfrac{s}{2}, & \text{if } C_1' C_2' \neq 0 \text{ and } \delta \le 180°,\\[6pt]
\dfrac{s + 360°}{2}, & \text{if } C_1' C_2' \neq 0,\ \delta > 180°,\ s < 360°,\\[6pt]
\dfrac{s - 360°}{2}, & \text{if } C_1' C_2' \neq 0,\ \delta > 180°,\ s \ge 360°.
\end{cases}
$$

> **Pitfall (mean hue is the #1 cause of wrong implementations).** When the two
> hues straddle the $0°/360°$ wrap, the naive average $(h_1'+h_2')/2$ is wrong by
> $180°$. The $\pm 360°$ correction fixes this. The achromatic branch
> ($C_1' C_2' = 0$) returns $s = h_1' + h_2'$ (**not** $s/2$): one chroma is zero
> so its hue is meaningless and the other hue passes through unchanged [S3].
>
> **Boundary case $\delta = 180°$ exactly.** This falls in the $\delta \le 180°$
> branch ($\overline{h'} = s/2$). The standard is not fully explicit here; this
> resolution follows [S3], which (for $\delta = 180°$ with $s < 360°$)
> deliberately yields $0°$ rather than the equally defensible $360°$. The choice
> is the source of one of the formula's small discontinuities.

---

## 4. Weighting functions $T$, $\Delta\theta$, $R_C$, $S_L$, $S_C$, $S_H$, $R_T$

**4.1 — Hue-dependent term $T$** (arguments in degrees):
$$T = 1 - 0.17\cos(\overline{h'} - 30°) + 0.24\cos(2\overline{h'}) + 0.32\cos(3\overline{h'} + 6°) - 0.20\cos(4\overline{h'} - 63°).$$

**4.2 — Rotation angle $\Delta\theta$** (degrees), localized near the blue hue $275°$:
$$\Delta\theta = 30\,\exp\!\left[-\left(\frac{\overline{h'} - 275°}{25}\right)^{\!2}\right].$$

**4.3 — Chroma-dependent rotation magnitude $R_C$:**
$$R_C = 2\sqrt{\dfrac{\overline{C'}^{\,7}}{\overline{C'}^{\,7} + 25^{7}}}.$$

**4.4 — Lightness weighting $S_L$:**
$$S_L = 1 + \dfrac{0.015\,(\overline{L'} - 50)^2}{\sqrt{20 + (\overline{L'} - 50)^2}}.$$

**4.5 — Chroma weighting $S_C$:**
$$S_C = 1 + 0.045\,\overline{C'}.$$

**4.6 — Hue weighting $S_H$:**
$$S_H = 1 + 0.015\,\overline{C'}\,T.$$

**4.7 — Rotation term $R_T$** (note the leading minus; $2\Delta\theta$ in degrees):
$$R_T = -\sin(2\,\Delta\theta)\,R_C.$$

> **Pitfall (powers and units).** The exponent $7$ and the constant $25^7$ in
> $G$, $R_C$ appear with the **7th power**; using a different exponent is a
> silent error. The $\exp$ argument in $\Delta\theta$ is dimensionless (degrees
> cancel), but $30$, $2\Delta\theta$, and the $T$-term constants are degrees —
> feed $\sin/\cos$ accordingly.

---

## 5. The CIEDE2000 color difference $\Delta E_{00}$

$$
\boxed{\;
\Delta E_{00} =
\sqrt{
\left(\dfrac{\Delta L'}{k_L S_L}\right)^{2}
+\left(\dfrac{\Delta C'}{k_C S_C}\right)^{2}
+\left(\dfrac{\Delta H'}{k_H S_H}\right)^{2}
+ R_T\left(\dfrac{\Delta C'}{k_C S_C}\right)\!\left(\dfrac{\Delta H'}{k_H S_H}\right)
}
\;}
$$

The final cross (rotation) term is the **only** part of the formula sensitive to
the sign of $\Delta C'$ and $\Delta H'$; it is significant only in the blue
region (mean hue $\approx 275°$) and vanishes whenever $\Delta C' = 0$ or
$\Delta H' = 0$ [S3].

---

## 6. Consolidated edge-case checklist

| # | Condition | Required behavior | Source |
|---|-----------|-------------------|--------|
| 1 | $a_i' = b_i' = 0$ | $h_i' \mathrel{:=} 0$ (hue indeterminate) | [S3] |
| 2 | `atan2` returns negative | add $360°$; keep $h_i' \in [0°,360°)$ | [S3] |
| 3 | $C_1' C_2' = 0$ | $\Delta h' = 0$ **and** $\overline{h'} = h_1' + h_2'$ | [S3] |
| 4 | $\lvert h_2' - h_1'\rvert > 180°$ | apply $\pm 360°$ to $\Delta h'$ using **signed** $d$ | [S1][S3] |
| 5 | hues straddle $0/360$ wrap | apply $\pm 360°$ to $\overline{h'}$ per sign of $s-360°$ | [S1][S3] |
| 6 | $\delta = 180°$ exactly | use arithmetic mean branch ($\overline{h'}=s/2$) | [S3] |
| 7 | $\Delta C',\Delta H'$ | keep **signed**; never use $\lvert\cdot\rvert$ | [S3] |
| 8 | all trig arguments | **degrees**, not radians | [S1] |
| 9 | $\Delta E_{00}(1,2)$ vs $(2,1)$ | must be equal — test by swapping inputs | [S3] |

Errors 1–7 are *not* exercised by the small worked-example set originally shipped
with the standard; they are caught only by the supplemental data in §7 [S3].

---

## 7. Validation test data

Selected pairs from the Sharma–Wu–Dalal supplemental set (Table I of [S3]),
$k_L = k_C = k_H = 1$. Results are quoted to 4 decimals; discrepancies even in
the 4th place indicate an implementation bug [S3]. Each pair is chosen to
exercise a specific branch.

| Pair | $L_1^*$ | $a_1^*$ | $b_1^*$ | $L_2^*$ | $a_2^*$ | $b_2^*$ | $\Delta E_{00}$ | Exercises |
|------|--------|--------|--------|--------|--------|--------|-----------|-----------|
| 1  | 50.0000 | 2.6772 | −79.7751 | 50.0000 | 0.0000 | −82.7485 | **2.0425** | blue / $R_T$ |
| 4  | 50.0000 | −1.3802 | −84.2814 | 50.0000 | 0.0000 | −82.7485 | **1.0000** | blue / $R_T$ |
| 8  | 50.0000 | −1.0000 | 2.0000 | 50.0000 | 0.0000 | 0.0000 | **2.3669** | $C_2'=0$ hue branch |
| 9  | 50.0000 | 2.4900 | −0.0010 | 50.0000 | −2.4900 | 0.0009 | **7.1792** | 180°-apart, $\Delta C'$ sign |
| 13 | 50.0000 | −0.0010 | 2.4900 | 50.0000 | 0.0009 | −2.4900 | **4.8045** | mean-hue wrap |
| 17 | 50.0000 | 2.5000 | 0.0000 | 73.0000 | 25.0000 | −18.0000 | **27.1492** | large difference |
| 18 | 50.0000 | 2.5000 | 0.0000 | 61.0000 | −5.0000 | 29.0000 | **22.8977** | large difference |
| 25 | 60.2574 | −34.0099 | 36.2677 | 60.4626 | −34.1751 | 39.4387 | **1.2644** | CIE-published |
| 33 | 6.7747 | −0.2908 | −2.4247 | 5.8714 | −0.0985 | −2.2286 | **0.6377** | dark / near-neutral |
| 34 | 2.0776 | 0.0795 | −1.1350 | 0.9033 | −0.0636 | −0.5514 | **0.9082** | dark / near-neutral |

The full 34-pair table (including intermediate values $a_i'$, $C_i'$, $h_i'$,
$\overline{h'}$, $G$, $T$, $S_L$, $S_C$, $S_H$, $R_T$) and reference Excel/MATLAB
implementations are published in [S3] and at the first author's site
(`ece.rochester.edu/~gsharma/ciede2000/`). A correct implementation must
reproduce all pairs and must be symmetric under input swap (§6 #9).

---

## 8. Reference computation order (summary)

```
Step 1:  C*_i,ab → C̄*_ab → G → a'_i → C'_i → h'_i        (§1)
Step 2:  ΔL' , ΔC' , Δh' → ΔH'                            (§2)
Step 3:  L̄' , C̄' , h̄'                                    (§3)
Step 4:  T , Δθ , R_C , S_L , S_C , S_H → R_T             (§4)
Step 5:  ΔE00                                            (§5)
```

## Full references

[S1] CIE. *Improvement to Industrial Colour-Difference Evaluation.* CIE Publication No. 142-2001. Central Bureau of the CIE, Vienna (2001).

[S2] Luo, M. R.; Cui, G.; Rigg, B. "The development of the CIE 2000 colour-difference formula: CIEDE2000." *Color Research & Application* **26**(4): 340–350 (2001). DOI: 10.1002/col.1049.

[S3] Sharma, G.; Wu, W.; Dalal, E. N. "The CIEDE2000 color-difference formula: Implementation notes, supplementary test data, and mathematical observations." *Color Research & Application* **30**(1): 21–30 (2005). DOI: 10.1002/col.20070.
