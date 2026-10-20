# قائمة فحص PSD — ملفات من Photoshop الخاص بك

الملفات التي اختُبرت بها لوحة حتى الآن ملفات اختبار عامة من مشروع مفتوح المصدر (انظر
`docs/psd-report.md`). هذه القائمة لصنع ملفات من Photoshop عندك، تغطي ما يهمّ مستخدمي لوحة.

## قبل البدء

- في Photoshop: **Preferences › File Handling › Maximize PSD and PSB File Compatibility = Always**.
  بدونها يحفظ Photoshop صورة مدمجة فارغة، فلا يبقى ما نقارن به.
- احفظ الملفات في `tests/psd-mine/` داخل مجلد المشروع (المجلد مستثنى من git).
- مقاس صغير يكفي (مثلًا 800×600)، 8-bit، RGB.

## الملفات (ملف لكل بند، والاسم بين القوسين)

1. **نص عربي** (`text-ar.psd`): سطر عنوان بخط عربي، وفقرة نص عربية داخل صندوق مضبوطة بالتساوي
   (Justify)، وسطر فيه كلمة إنجليزية وأرقام وسط العربي.
2. **مجموعات وأنماط مزج** (`groups.psd`): مجموعتان، إحداهما Pass Through والأخرى Normal بشفافية 60%،
   وداخلهما طبقات بأنماط Multiply وScreen وOverlay وSoft Light وColor.
3. **أقنعة** (`masks.psd`): طبقة بقناع مرسوم بفرشاة ناعمة، وطبقة بقناع متجهي (Pen)، وطبقة بالاثنين،
   وقناع معطّل (Shift+نقر)، وقناع بـ Density 50% وFeather 10px من نافذة Properties.
4. **قص (Clipping Mask)** (`clipping.psd`): صورة مقصوصة على نص، وطبقة ضبط مقصوصة على طبقة واحدة.
5. **طبقات ضبط** (`adjust-*.psd`): ملف لكل نوع، فوق صورة ملونة (صورة طبيعة أو تدرج ألوان):
   Levels، Curves، Hue/Saturation (ومرة مع Colorize)، Brightness/Contrast، Exposure، Vibrance،
   Color Balance، Black & White، Gradient Map، Invert، Threshold.
   هذه أهم ملفات القائمة: معادلات لوحة مكتوبة من صفحات المساعدة العامة، وهذه الملفات تقيس الفرق.
6. **أنماط الطبقة** (`styles.psd`): شكل أو نص عليه Drop Shadow، وآخر Inner Shadow، وOuter Glow،
   وStroke (خارج، داخل، وسط)، وColor Overlay، وGradient Overlay. ثم طبقة بـ Fill 0% مع Stroke.
7. **أشكال متجهية** (`shapes.psd`): مستطيل وشكل بيضاوي وشكل مرسوم بـ Pen بلون تعبئة.
8. **كائن ذكي** (`smart.psd`): صورة مضافة كـ Smart Object ومصغّرة ومدوّرة.
9. **16-bit** (`deep.psd`): أي ملف من أعلاه محوّل إلى 16 Bits/Channel.

## الفحص

1. شغّل لوحة في وضع التطوير مع `LAWHA_HARNESS=1`، ثم:

   ```bash
   tools/front.sh on && node tools/psd-report.mjs tests/psd-mine && tools/front.sh off
   ```

   يكتب `tests/psd-mine/report.md`: مقارنة كل ملف بصورة Photoshop المدمجة، وما لم يُنقل.
2. افتح كل ملف في لوحة بنفسك: هل الطبقات والأسماء والترتيب كما في Photoshop؟ هل النص العربي
   قابل للتحرير؟ هل التحذير الذي يظهر عند الفتح صادق؟
3. من لوحة: **ملف › حفظ نسخة بصيغة PSD**، ثم افتح النسخة في Photoshop: هل الطبقات والأقنعة
   والأنماط وطبقات الضبط ما زالت قابلة للتحرير هناك؟
4. أرسل `report.md` وما لاحظته (لقطات شاشة من Photoshop ولوحة لنفس الملف تكفي).

---

# PSD checklist — files from your own Photoshop

Lawha has so far been tested on public test files from an open-source project (see
`docs/psd-report.md`). This list is for making files in your own Photoshop that cover what Lawha's
users care about.

## Before you start

- In Photoshop: **Preferences › File Handling › Maximize PSD and PSB File Compatibility = Always**.
  Without it Photoshop stores a blank flattened image, and there is nothing to compare with.
- Save the files in `tests/psd-mine/` in the project folder (ignored by git).
- Small is fine (say 800×600), 8-bit, RGB.

## The files (one per item; suggested names in brackets)

1. **Arabic text** (`text-ar.psd`): a heading line in an Arabic font, a justified Arabic paragraph in
   a text box, and a line mixing Arabic with an English word and digits.
2. **Groups and blend modes** (`groups.psd`): two groups, one Pass Through and one Normal at 60%
   opacity, with layers inside in Multiply, Screen, Overlay, Soft Light and Color.
3. **Masks** (`masks.psd`): a layer with a soft-brush mask, one with a vector mask (Pen), one with
   both, a disabled mask (Shift-click), and a mask with Density 50% and Feather 10 px (Properties).
4. **Clipping masks** (`clipping.psd`): a photo clipped to text, and an adjustment clipped to one layer.
5. **Adjustment layers** (`adjust-*.psd`): one file per kind, over a colorful photo or color ramp:
   Levels, Curves, Hue/Saturation (and once with Colorize), Brightness/Contrast, Exposure,
   Vibrance, Color Balance, Black & White, Gradient Map, Invert, Threshold.
   These matter most: Lawha's formulas are written from public help pages, and these files measure
   the difference.
6. **Layer styles** (`styles.psd`): a shape or text with Drop Shadow, another with Inner Shadow,
   Outer Glow, Stroke (outside, inside, center), Color Overlay and Gradient Overlay; then a layer
   with Fill 0% and a Stroke.
7. **Vector shapes** (`shapes.psd`): a rectangle, an ellipse and a Pen shape with a fill color.
8. **Smart object** (`smart.psd`): an image placed as a Smart Object, scaled and rotated.
9. **16-bit** (`deep.psd`): any of the above converted to 16 Bits/Channel.

## Checking

1. Run Lawha in development with `LAWHA_HARNESS=1`, then:

   ```bash
   tools/front.sh on && node tools/psd-report.mjs tests/psd-mine && tools/front.sh off
   ```

   This writes `tests/psd-mine/report.md`: each file against Photoshop's flattened image, and what
   wasn't carried over.
2. Open each file in Lawha yourself: are the layers, names and order as in Photoshop? Is the Arabic
   text editable? Is the warning shown on opening accurate?
3. In Lawha: **File › Save a Copy as PSD**, then open the copy in Photoshop: are the layers, masks,
   styles and adjustment layers still editable there?
4. Send `report.md` and what you noticed (screenshots of the same file in Photoshop and in Lawha are
   enough).
