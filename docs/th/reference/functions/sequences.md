<!-- translated-from: docs/ja/reference/functions/sequences.md @ 8c7bff99b2cddddb57736d0567933bc024a5a467 -->
# คู่ S-expression และลำดับ

คู่ generic `cons-cell` ข้อมูล S-expression `Sexpr` สัญลักษณ์ ฟังก์ชันของลำดับที่เขียนบน
`Iter` และฟังก์ชันอันดับสูง

## 1. คู่ (`cons-cell<A,B>`)

`cons`/`car`/`cdr` คือตัวสร้างและตัวเข้าถึงฟิลด์ของ **ชนิดคู่ generic
`cons-cell<A,B>`** (`defstruct` ในไลบรารีมาตรฐาน) อ่านฟิลด์ได้ทั้งเป็น
`variable::car`/`variable::cdr` (ไวยากรณ์ตัวเข้าถึงของ `defstruct` ใน
[เอกสารอ้างอิงไวยากรณ์](../syntax.md#36-defstruct--struct-ชนิดที่ผู้ใช้นิยาม)) หรือเป็น
`(car variable)`/`(cdr variable)` หากต้องการเปลี่ยน ให้ใช้ `(setf variable::car v)`/`(setf variable::cdr v)`

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `cons` | `(cons a b)` | `(A,B)→cons-cell<A,B>` | สร้างคู่ |
| `car` | `(car p)` | `cons-cell<A,B>→A` | สมาชิกตัวแรก |
| `cdr` | `(cdr p)` | `cons-cell<A,B>→B` | ส่วนที่เหลือ |

`cons-cell` ยังใช้แทนไวยากรณ์ tuple ด้วย ฟังก์ชัน CL ที่คืนค่าหลายค่า (ผลหารและเศษของ `floor`
ค่าและตำแหน่งของ `read-from-string` และอื่น ๆ) คืน `cons-cell` ในภาษานี้

## 2. ข้อมูล S-expression (`Sexpr`)

ชนิดข้อมูล `Sexpr` ที่ `read` คืนมี 19 variant:
`int | i8 | i16 | i32 | u8 | u16 | u32 | f32 | f64 | char | bool | sym | str | cons | ratio | path | vector | array | tuple`
`vector` และ `array` คือข้อมูลที่เขียนเป็น `#(..)` และ `#nA(..)` ([เอกสารอ้างอิงไวยากรณ์](../syntax.md#1-องค์ประกอบทางศัพท์)) ซึ่งถือ `Vector<Option<Sexpr>>` และ `Array<Option<Sexpr>>` ตามลำดับ `len`, `get` และอื่น ๆ ใช้กับ `v` ที่ `(vector v)` ผูกไว้ได้โดยตรง
`tuple` คือข้อมูลที่เขียนด้วย `#{..}` และ `v` ที่ `(tuple v)` ผูกไว้คือ `Vector<Option<Sexpr>>` ใหม่ที่เรียงสมาชิก (เพื่อรับทูเพิลทุกความยาวด้วยชนิดเดียว)
เซลล์ S-expression ถูกจัดการด้วยฟังก์ชัน `sexpr-*` ไม่ใช่ด้วย `cons`/`car`/`cdr` ทั่วไปของบทที่ 1
ใช้หลัก ๆ ในตัวเนื้อหาของ `defmacro` เพื่อสร้างและแยกฟอร์ม

**ชนิดของข้อมูล S-expression คือ `Option<Sexpr>`** ลิสต์ว่างไม่ใช่ variant ของ `Sexpr` แต่เป็น
`none` ของ `Option` และ `Sexpr` เองหมายถึง "S-expression ที่ไม่ว่าง" ดังนั้นฟังก์ชัน `sexpr-*`
รับและคืน `Option<Sexpr>`

- `()` คือลิสต์ว่างในที่ที่คาดหวัง `Option<Sexpr>` (เขียนเป็น
  `(Option::none)` ได้ด้วย)
- `Sexpr` ขยายโดยปริยายในที่ที่คาดหวัง `Option<Sexpr>` (ไม่มีการแปลงตอนรัน) ทิศตรงข้าม
  คือการใช้ `Option<Sexpr>` เป็น `Sexpr` อ้างว่า "นี่ไม่ใช่ลิสต์ว่าง"
  จึงต้องระบุอย่างชัดเจนด้วย `match` หรือ `unwrap`
- ใน `match` เขียน 19 variant ของ `Sexpr` และ `none` **แบบแบนในรายการกิ่งเดียวกัน** ได้
  ([เอกสารอ้างอิงไวยากรณ์](../syntax.md#43-match--การจับคู่รูปแบบ))

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `sexpr-cons` | `(sexpr-cons a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | สร้างเซลล์ `Sexpr` |
| `sexpr-car` | `(sexpr-car s)` | `Option<Sexpr>→Option<Sexpr>` | สมาชิกตัวแรก **ลิสต์ว่างสำหรับลิสต์ว่าง** (เหมือนใน CL) panic กับ atom ที่ไม่ใช่ `Cons` |
| `sexpr-cdr` | `(sexpr-cdr s)` | `Option<Sexpr>→Option<Sexpr>` | ส่วนที่เหลือ **ลิสต์ว่างสำหรับลิสต์ว่าง** (เหมือนใน CL) panic กับ atom ที่ไม่ใช่ `Cons` |
| `sexpr-consp` | `(sexpr-consp s)` | `Option<Sexpr>→bool` | เป็น `Cons` หรือไม่ |
| `sexpr-null` | `(sexpr-null s)` | `Option<Sexpr>→bool` | เป็นลิสต์ว่างหรือไม่ |
| `sexpr-atom` | `(sexpr-atom s)` | `Option<Sexpr>→bool` | ไม่ใช่ `Cons` หรือไม่ |
| `sexpr-symp` | `(sexpr-symp s)` | `Option<Sexpr>→bool` | เป็น `Sym` (สัญลักษณ์) หรือไม่ |
| `sexpr-int` | `(sexpr-int s)` | `Option<Sexpr>→int` | เนื้อหาของ variant `int` (fixnum หรือ bignum) panic กับชนิดอื่น |
| `sexpr-i8` `sexpr-i16` `sexpr-i32` `sexpr-u8` `sexpr-u16` `sexpr-u32` | `(sexpr-i32 s)` | `Option<Sexpr>→W` | เนื้อหาของ variant ความกว้างนั้น panic กับชนิดอื่น |
| `sexpr-f32` `sexpr-f64` | `(sexpr-f64 s)` | `Option<Sexpr>→f32` / `→f64` | เนื้อหาของ variant จำนวนทศนิยม panic กับชนิดอื่น |
| `sexpr-char` | `(sexpr-char s)` | `Option<Sexpr>→char` | เนื้อหาของ `Char` panic กับชนิดอื่น |
| `sexpr-bool` | `(sexpr-bool s)` | `Option<Sexpr>→bool` | เนื้อหาของ `Bool` panic กับชนิดอื่น |
| `sexpr-str` | `(sexpr-str s)` | `Option<Sexpr>→string` | เนื้อหาของ `Str` panic กับชนิดอื่น |
| `sexpr-sym-name` | `(sexpr-sym-name s)` | `Option<Sexpr>→string` | ชื่อของ `Sym` panic กับชนิดอื่น |
| `eq` `eql` | `(op a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | การเปรียบเทียบเอกลักษณ์ (`Cons`/`Str` เปรียบเทียบเอกลักษณ์ของออบเจ็กต์ ที่เหลือเปรียบเทียบค่า) |
| `equal` | `(equal a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | ความเท่ากันเชิงโครงสร้าง (`Cons` แบบเวียนเกิด `Str` ตามเนื้อหา) |
| `equalp` | `(equalp a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | เหมือน `equal` บวกการเปรียบเทียบแบบไม่แยกตัวพิมพ์ใหญ่เล็กและการเปรียบเทียบตัวเลขข้ามชนิด |
| `sexpr-append` | `(sexpr-append a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | ต่อลิสต์ `Sexpr` สองลิสต์ (แบบไม่ทำลาย) `,@` ขยายเป็นสิ่งนี้ |
| `sexpr-map` | `(sexpr-map f lst)` | `((fn (Option<Sexpr>) Option<Sexpr>),Option<Sexpr>)→Option<Sexpr>` | ลิสต์ `Sexpr` ใหม่ที่ใช้ `f` กับแต่ละสมาชิกของลิสต์ `Sexpr` (`map` ของบทที่ 4 ใช้กับ `Iter` และเดินผ่านลิสต์ `Sexpr` ไม่ได้) |

มีตัวเข้าถึงตัวเลขเก้าตัว หนึ่งตัวต่อชนิด เพราะ `Sexpr` คือ "ที่เดียวที่ชนิด
ของค่าไม่ถูกเขียนไว้ที่อื่น" `u8` ที่ใส่ใน `Sexpr` เข้าไปเป็น variant `u8` และออกมา
ได้ด้วย `(sexpr-u8 s)` เท่านั้น การส่งให้ `(sexpr-int s)` จะ panic มันไม่ขยายคำตอบโดยไม่แจ้ง
จำนวนเต็มในข้อมูลที่อ่านมา (`'(1 2 3)`, อาร์กิวเมนต์ของแมโคร) เป็น variant `int`
และอ่านด้วย `(sexpr-int s)`

ลิสต์ `Sexpr` ไม่มีการดำเนินการแบบทำลายอย่าง `rplaca`/`nconc` เซลล์ `Sexpr` เปลี่ยนแปลง
ไม่ได้หลังสร้าง

## 3. สัญลักษณ์

`symbol` คือชนิดของสัญลักษณ์เอง แปลงโดยปริยายในที่ที่ต้องการ `Sexpr` แต่ไม่แปลง
อัตโนมัติในทิศตรงข้าม

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `symbol->string` | `(symbol->string s)` | `symbol→string` | ดึงชื่อของสัญลักษณ์ออกมา |
| `string->symbol` | `(string->symbol s)` | `string→symbol` | สร้างสัญลักษณ์จากสตริง (intern) |
| `keywordp` | `(keywordp s)` | `symbol→bool` | เป็นคีย์เวิร์ด (`:name`) หรือไม่ เครื่องหมายทวิภาคเป็นส่วนของชื่อ ดังนั้นการทดสอบดูที่อักขระตัวแรก ([เอกสารอ้างอิงไวยากรณ์](../syntax.md#1-องค์ประกอบทางศัพท์)) |

สำหรับ `gensym` ดู[แมโคร](system.md#8-แมโคร)

## 4. ฟังก์ชันของลำดับบน `Iter`

ฟังก์ชันของลำดับเป็น **ฟังก์ชัน generic บน trait `Iter`** จากคอลเลกชัน ให้รับตัววนซ้ำด้วย
`(iter coll)` แล้วส่งมัน (`Vector<T>` / `HashTable<K,V>` / `Array<T>` รองรับสิ่งนี้
ลิสต์ `Sexpr` ไม่ implement `Iter` ดังนั้นฟังก์ชันเหล่านี้ใช้กับมันไม่ได้) **คอลเลกชันผลลัพธ์
คืนเป็น `Vector` ใหม่** `Iter<A>` ในตารางหมายถึง "การ implement ใดก็ได้ของ
`Iter` ที่ `Item` เป็น `A`" หากต้องการเดินผ่าน `Vector` ที่คืนมาอีกครั้ง ให้ส่ง `(iter result)`

ฟังก์ชันที่รับ predicate (สอดคล้องกับตระกูล `-if` ของ CL):

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `map` | `(map it f)` | `(Iter<A>,(fn (A) U))→Vector<U>` | การแมป |
| `filter` | `(filter it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | เฉพาะสมาชิกที่ตรงตาม predicate |
| `remove-if` | `(remove-if it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | ลบสมาชิกที่ตรงตาม predicate |
| `find-if` | `(find-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<A>` | สมาชิกแรกที่ตรงตาม predicate |
| `position-if` | `(position-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<int>` | ตำแหน่งแรกที่ตรงตาม predicate |
| `count-if` | `(count-if it pred)` | `(Iter<A>,(fn (A) bool))→int` | มีกี่ตัวที่ตรงตาม predicate |
| `every` | `(every it pred)` | `(Iter<A>,(fn (A) bool))→bool` | ทุกสมาชิกตรงตาม predicate หรือไม่ |
| `any` | `(any it pred)` | `(Iter<A>,(fn (A) bool))→bool` | มีสมาชิกใดตรงตาม predicate หรือไม่ (สอดคล้องกับ `some` ของ CL; ชื่อที่ไม่ชนกับตัวสร้าง `Some`) |
| `foldl` | `(foldl it f init)` | `(Iter<A>,(fn (B A) B),B)→B` | การพับซ้าย |
| `foldr` | `(foldr it f init)` | `(Iter<A>,(fn (A B) B),B)→B` | การพับขวา |

การทำดัชนี ความยาว และการตัดส่วน:

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `length` | `(length it)` | `Iter<A>→int` | จำนวนสมาชิก |
| `append` | `(append a b ...)` | `(Iter<A>,Iter<A>,...)→Vector<A>` | ต่อตัววนซ้ำ ให้ได้ตั้งแต่สามตัวขึ้นไป |
| `concatenate` | `(concatenate 'vector it ...)` / `(concatenate 'string s ...)` | `→Vector<A>` / `→string` | `concatenate` ของ CL ชนิดผลลัพธ์เขียนเป็น **literal สัญลักษณ์ที่ quote** (CL ใช้ type specifier ตอนรัน) `'vector` รับหนึ่งตัวขึ้นไป `'string` รับศูนย์ตัวขึ้นไป (`""` เมื่อศูนย์) ไม่ครอบคลุมลิสต์ `Sexpr` (ใช้ `sexpr-append`) |
| `reverse` | `(reverse it)` | `Iter<A>→Vector<A>` | การกลับลำดับ (แบบไม่ทำลาย) |
| `nth` | `(nth n it)` | `(int,Iter<A>)→Option<A>` | สมาชิกตัวที่ `n` (`None` เมื่อเกินช่วง) |
| `elt` | `(elt it n)` | `(Iter<A>,int)→Option<A>` | `nth` ที่อาร์กิวเมนต์สลับกัน |
| `take` | `(take it n)` | `(Iter<A>,int)→Vector<A>` | `n` สมาชิกแรก |
| `subseq` | `(subseq it start end)` | `(Iter<A>,int,int)→Vector<A>` | `[start,end)` (`end` ถูกจำกัดที่ความยาว) |
| `last` | `(last it)` | `Iter<A>→Option<A>` | **สมาชิก** ตัวสุดท้าย (ไม่ใช่ "เซลล์สุดท้าย" อย่างใน CL) |
| `butlast` | `(butlast it)` | `Iter<A>→Vector<A>` | ทุกตัวยกเว้นตัวสุดท้าย |

ฟังก์ชันที่ต้องการ bound `Eq` / `Ord` (เปรียบเทียบผ่าน trait แทน predicate;
[Trait มาตรฐาน](traits.md#2-eq--ord-การเปรียบเทียบ)):

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `member` | `(member x it)` | `(A,Iter<A>)→bool` where `Eq A` | มีสมาชิกที่เท่ากับ `x` หรือไม่ (ต่างจาก CL เป็น `bool` ไม่ใช่ส่วนที่เหลือของลิสต์) |
| `find` | `(find x it)` | `(A,Iter<A>)→Option<A>` where `Eq A` | สมาชิกแรกที่เท่ากับ `x` |
| `position` | `(position x it)` | `(A,Iter<A>)→Option<int>` where `Eq A` | ตำแหน่งแรกที่เท่ากับ `x` |
| `count` | `(count x it)` | `(A,Iter<A>)→int` where `Eq A` | มีกี่สมาชิกที่เท่ากับ `x` |
| `sort` | `(sort it cmp)` | `(Iter<A>,(fn (A A) bool))→Vector<A>` | `(sort sequence predicate)` ของ CL การเรียงแบบเสถียรและไม่ทำลาย `cmp` เป็น `true` เมื่อ "อาร์กิวเมนต์แรกมาก่อนอาร์กิวเมนต์ที่สองอย่างเคร่งครัด" |
| `assoc` | `(assoc k it)` | `(K,Iter<cons-cell<K,V>>)→Option<cons-cell<K,V>>` where `Eq K` | คู่แรกที่ `car` เท่ากับ `k` ดึงค่าออกด้วย `(cdr p)` |

ฟังก์ชันเหล่านี้และหลายตัวในบทที่ 5 รับอาร์กิวเมนต์คีย์เวิร์ดของ CL `:key` / `:test` /
`:test-not` / `:start` / `:end` / `:from-end` / `:count` ด้วย (บทที่ 6)

## 5. ฟังก์ชันของลำดับที่เหลือของ CL

ทั้งหมดเป็นฟังก์ชัน generic บน `Iter` เหมือนบทที่ 4 คอลเลกชันผลลัพธ์คืนเป็น `Vector`
ใหม่

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `first`…`tenth` | `(first it)` | `Iter<A>→Option<A>` | ดัชนีที่มีชื่อของ CL |
| `rest` | `(rest it)` | `Iter<A>→Vector<A>` | ทุกตัวยกเว้นตัวแรก (`Vector` ใหม่ ไม่ใช่หางที่ใช้ร่วมกัน) |
| `copy-seq` | `(copy-seq it)` | `Iter<A>→Vector<A>` | ทำตัววนซ้ำให้เป็น `Vector` จริง (`copy-seq`/`copy-list` ของ CL) |
| `revappend` | `(revappend a b)` | `(Iter<A>,Iter<A>)→Vector<A>` | `a` ที่กลับลำดับ ตามด้วย `b` |
| `Vector::filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `x` จำนวน `n` ตัว (`make-list`/`make-sequence` ของ CL) เหมือน `Vector::new` อาร์กิวเมนต์ชนิดมาจากชนิดที่คาดหวัง ดังนั้น `let` เปล่า ๆ ต้องใช้ `the` |
| `member-if` `member-if-not` | `(member-if it pred)` | `(Iter<A>,(fn (A) bool))→bool` | เหมือน `member` เป็น **`bool`** (ตัววนซ้ำไม่มีหางให้คืน) |
| `notany` `notevery` | `(notany it pred)` | `(Iter<A>,(fn (A) bool))→bool` | นิเสธของ `any`/`every` |
| `find-if-not` `position-if-not` `count-if-not` `remove-if-not` | `(op it pred)` | ชนิดเดียวกับเวอร์ชันบวก | เวอร์ชันที่ predicate ถูกนิเสธ |
| `remove` | `(remove x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | ลบตามค่า |
| `remove-duplicates` | `(remove-duplicates it)` | `Iter<A>→Vector<A>` where `Eq A` | ลบรายการซ้ำ เหมือนใน CL **ตัวที่ปรากฏครั้งสุดท้ายถูกเก็บไว้** (`:from-end true` เก็บตัวแรก) |
| `substitute` `substitute-if` | `(substitute new old it)` | `(A,A,Iter<A>)→Vector<A>` | แทนที่ตามค่า / predicate |
| `assoc-if` `rassoc` `rassoc-if` | `(rassoc v it)` | บน `Iter<cons-cell<K,V>>` | เวอร์ชัน predicate และเวอร์ชันฝั่งค่าของ `assoc` |
| `acons` | `(acons k v it)` | `(K,V,Iter<cons-cell<K,V>>)→Vector<cons-cell<K,V>>` | เพิ่มคู่ที่ด้านหน้า |
| `pairlis` | `(pairlis ks vs)` | `(Iter<K>,Iter<V>)→Vector<cons-cell<K,V>>` | จับคู่สองลำดับ หยุดที่ตัวที่สั้นกว่า |
| `map2` | `(map2 a b f)` | `(Iter<A>,Iter<B>,(fn (A B) U))→Vector<U>` | `mapcar` ของ CL บนหลายลำดับ หยุดที่ตัวที่สั้นกว่า |
| `mapc` | `(mapc it f)` | `(Iter<A>,(fn (A) ()))→()` | การแมปเพื่อผลข้างเคียง |
| `mapcan` | `(mapcan it f)` | `(Iter<A>,(fn (A) Vector<U>))→Vector<U>` | แมปแล้วต่อกัน |
| `maplist` | `(maplist it f)` | `(Iter<A>,(fn (Vector<A>) U))→Vector<U>` | แมปบน **หาง** ที่ต่อเนื่องกัน |
| `mapl` | `(mapl it f)` | `(Iter<A>,(fn (Vector<A>) ()))→()` | แมปบนหางเพื่อผลข้างเคียง (คู่ของ `maplist` ที่เทียบกับ `mapc`) |
| `mapcon` | `(mapcon it f)` | `(Iter<A>,(fn (Vector<A>) Vector<U>))→Vector<U>` | แมปบนหางแล้วต่อกัน (คู่ของ `maplist` ที่เทียบกับ `mapcan`) |
| `search` | `(search it sub)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | ตำแหน่งที่ `sub` ปรากฏครั้งแรก หากตัวรับเป็น `string` จะเลือกเมทอดของ `string` ([สตริง](collections.md#1-สตริง-string)) |
| `mismatch` | `(mismatch a b)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | ตำแหน่งแรกที่ต่างกัน `none` หากเท่ากัน |
| `merge` | `(merge a b less)` | `(Iter<A>,Iter<A>,(fn (A A) bool))→Vector<A>` | การผสาน CL ต้องการอินพุตที่เรียงแล้ว ที่นี่เรียงลำดับผลการต่อกัน |
| `adjoin` | `(adjoin x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | เพิ่ม `x` **ที่ด้านหน้า** หากยังไม่มี |
| `union` `intersection` `set-difference` `set-exclusive-or` | `(op a b)` | `(Iter<A>,Iter<A>)→Vector<A>` where `Eq A` | การดำเนินการเซต CL ไม่กำหนดลำดับ ที่นี่เสถียร **ตามลำดับที่ปรากฏครั้งแรก** |
| `subsetp` | `(subsetp a b)` | `(Iter<A>,Iter<A>)→bool` where `Eq A` | การเป็นสับเซต |
| `tailp` `ldiff` | `(tailp tail whole)` | `(Iter<A>,Iter<A>)→bool` / `→Vector<A>` | เป็นส่วนต่อท้ายหรือไม่ / ส่วนก่อนส่วนต่อท้าย CL ถามเกี่ยวกับ **โครงสร้างที่ใช้ร่วมกัน** แต่ไม่มีโครงสร้างให้ใช้ร่วมกัน จึงถามเกี่ยวกับส่วนต่อท้าย **ในฐานะค่า** |
| `seq-equals` | `(seq-equals a b)` | `(Vector<A>,Vector<A>)→bool` where `Eq A` | ความเท่ากันทีละสมาชิก `Vector<T>` เองไม่ implement `Eq` |
| `caar`…`cddddr` | `(cadr p)` | บนคู่ที่ซ้อนกัน | 28 ฟังก์ชันของ CL เดินผ่าน **คู่ ไม่ใช่ลิสต์**: `cadr` รับ `cons-cell<A,cons-cell<B,C>>` |

สิ่งที่ CL มีแต่ภาษานี้ไม่มี: `list*` (ไม่มีแนวคิดของลิสต์ไม่สมบูรณ์ที่หางถูกแทนที่),
`copy-tree`/`copy-alist`/`sublis`/`subst`/`subst-if` (ไม่มีชนิดใดอธิบายการเดินผ่านต้นไม้
ที่ไม่เป็นเนื้อเดียวกันที่ความลึกใดก็ได้ สำหรับต้นไม้ของ `Sexpr` `equal` สอดคล้องกับ `tree-equal`)
ตระกูล property list `getf`/`get-properties`/`symbol-plist`/`remprop` (ไม่มี
การแสดงผลเป็นลิสต์ไม่มีชนิดที่สลับคีย์กับค่า `assoc` (association list) หรือ
`HashTable` ทำหน้าที่เดียวกัน) และฟังก์ชันที่แปลงระหว่าง `Vector<T>` กับลิสต์ `Sexpr` (
สมาชิกของลิสต์ `Sexpr` แต่ละตัวมีชนิดต่างกันได้ จึงเขียนด้วยชนิดสมาชิกเดียว `T` ไม่ได้)

## 6. อาร์กิวเมนต์คีย์เวิร์ด

ฟังก์ชันของบทที่ 4 และ 5 รับคีย์เวิร์ดของลำดับของ CL `:key` / `:test` / `:test-not` /
`:start` / `:end` / `:from-end` / `:count` ทั้งหมด **ไม่บังคับ**

| คีย์เวิร์ด | ชนิด | ความหมาย |
|---|---|---|
| `:key` | `(fn (A) A)` | การฉายที่ใช้กับแต่ละสมาชิกก่อนเปรียบเทียบหรือทดสอบ |
| `:test` | `(fn (A A) bool)` | การทดสอบความเท่ากันที่ใช้แทน `equals` จาก bound `Eq` อาร์กิวเมนต์แรกคือ **รายการที่กำลังค้นหา** อาร์กิวเมนต์ที่สองคือสมาชิก (หลัง `:key`) ลำดับเดียวกับ CL |
| `:test-not` | `(fn (A A) bool)` | นิเสธของ `:test` |
| `:start` `:end` | `int` | หน้าต่าง `[start, end)` ที่สแกน ดัชนีสัมพัทธ์กับทั้งลำดับ |
| `:from-end` | `bool` | การค้นหาตอบด้วยรายการที่ตรงกัน **ตัวสุดท้าย** เมื่อรวมกับ `:count` สมาชิกที่ได้รับผลกระทบจะนับจากท้าย |
| `:count` | `int` | จำนวนสมาชิกสูงสุดที่ตระกูล `remove` / `substitute` กระทบ |

ฟังก์ชันใดรับคีย์เวิร์ดใดเป็นไปตาม CL:

| ฟังก์ชัน | คีย์เวิร์ดที่รับ |
|---|---|
| `find` `position` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `count` | `:key` `:test` `:test-not` `:start` `:end` |
| `member` `adjoin` | `:key` `:test` `:test-not` |
| `remove` `substitute` | ทั้งหมดข้างบน (รวม `:count`) |
| `remove-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `find-if` `find-if-not` `position-if` `position-if-not` | `:key` `:start` `:end` `:from-end` |
| `count-if` `count-if-not` | `:key` `:start` `:end` |
| `member-if` `member-if-not` | `:key` |
| `remove-if` `remove-if-not` `substitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `assoc` `rassoc` | `:key` `:test` `:test-not` (`:key` ของ `assoc` ใช้กับ `car` ส่วนของ `rassoc` ใช้กับ `cdr`) |
| `assoc-if` `rassoc-if` | `:key` |
| `sort` `merge` | `:key` |
| `union` `intersection` `set-difference` `set-exclusive-or` `subsetp` | `:key` `:test` `:test-not` |
| `search` | `:key` `:test` `:test-not` `:from-end` `:start` `:end` `:sub-start` `:sub-end` |
| `mismatch` | `:key` `:test` `:test-not` `:from-end` `:start1` `:end1` `:start2` `:end2` |

```lisp
(find 2 (iter v) :key (lambda ((x int)) int (abs x)))   ; → (some -2)
(remove 2 (iter v) :count 1 :from-end true)             ; removes only one, from the end
(position 3 (iter v) :start 1)                          ; the index is relative to the whole sequence
```

**ความแตกต่างจาก CL**:

1. **การฉายของ `:key` อยู่ภายในชนิดของสมาชิก** (`(fn (A) A)`) ฉายไปยัง
   ชนิดอื่นอย่างใน CL ไม่ได้: ตัวแปรชนิดเพิ่มเติมจะกำหนดไม่ได้เมื่อละอาร์กิวเมนต์
   ในที่ที่ต้องการการฉายไปยังชนิดอื่น ให้ส่ง lambda ให้ตระกูล `-if` แทน
   (`(find-if it (lambda ((p ...)) bool (= (car p) 3)))`)
2. **ในการค้นหาตามรายการ `:key` ใช้กับสมาชิกเท่านั้น** (ไม่ใช่กับรายการที่ค้นหา)
   กฎเดียวกับ `find`/`position`/`count`/`member`/`remove`/`substitute` ของ CL ในการดำเนินการ
   เซต ทั้งสองฝั่งเป็นสมาชิก จึงใช้กับทั้งสอง
3. **เฉพาะคีย์เวิร์ดของ `search` ที่ตั้งชื่อแทนการใช้หมายเลข** ใน CL `:start1`/`:end1` ใช้กับ
   **รูปแบบ (pattern)** และ `:start2`/`:end2` กับลำดับที่ค้นหา ในภาษานี้ตัวรับ
   มาก่อน ดังนั้นหมายเลขเดียวกันจะหมายถึงตรงข้าม และโดยไม่แจ้งด้วย `:start`/`:end`
   สำหรับตัวรับและ `:sub-start`/`:sub-end` สำหรับ pattern ดังนั้น `:start1` ที่พลั้งเผลอ
   ให้ข้อผิดพลาด "unknown keyword" `mismatch` และ `replace` มีลำดับอาร์กิวเมนต์เดียวกับ CL จึง
   คงหมายเลขของ CL

## 7. การดำเนินการแบบทำลาย

เมทอดของ `Vector<T>` **แก้ไขตัวรับและคืนตัวรับเอง** ดังนั้น `(nreverse v)`
เขียนเหมือน `reverse` และ `v` เองก็ถูกกลับลำดับด้วย

| ชื่อ | รูปแบบ | คำอธิบาย |
|---|---|---|
| `nreverse` | `(nreverse v)` | กลับลำดับในที่เดิม |
| `delete` `delete-if` `delete-if-not` `delete-duplicates` | `(delete v x)` | เวอร์ชันในที่เดิมของ `remove` / `remove-if` / `filter` / `remove-duplicates` |
| `nsubstitute` `nsubstitute-if` | `(nsubstitute v new old)` | เวอร์ชันในที่เดิมของตระกูล `substitute` |
| `nbutlast` | `(nbutlast v)` | ทิ้งสมาชิกตัวสุดท้าย |
| `fill` | `(fill v x)` | ตั้งทุกสมาชิกเป็น `x` ความยาวไม่เปลี่ยน |
| `replace` | `(replace v src)` | เขียนทับจากด้านหน้าด้วยสมาชิกของ `src` `(min (len v) (len src))` สมาชิก |
| `map-into` | `(map-into v src f)` | `v[i] = (f src[i])` จำนวนเท่าข้างบน |
| `nconc` | `(nconc v w)` | ต่อสมาชิกของ `w` เข้ากับ `v` ต่างจาก CL ตรงที่ **ไม่เขียนทับโครงสร้างที่ใช้ร่วมกัน** (`w` ไม่ได้รับผลกระทบ) |
| `nreconc` | `(nreconc v w)` | `(nconc (nreverse v) w)` |
| `set-contents` | `(set-contents v src)` | แทนที่เนื้อหาของ `v` ด้วย `src` (ความยาวก็เปลี่ยนด้วย) |
| `rplaca` `rplacd` | `(rplaca p x)` | เขียนทับ `car`/`cdr` ของ `cons-cell` และคืนเซลล์เอง |

คีย์เวิร์ดที่รับ:

| เวอร์ชันแบบทำลาย | คีย์เวิร์ดที่รับ |
|---|---|
| `delete` `nsubstitute` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` `:count` |
| `delete-if` `delete-if-not` `nsubstitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `delete-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `fill` | `:start` `:end` |
| `replace` | `:start1` `:end1` `:start2` `:end2` (ตัวรับคือ `sequence-1` ของ CL) |

`vector-push-extend`/`vector-pop` ก็คือ `push`/`pop` ของ `Vector<T>` เท่านั้น `Vector<T>` เติบโตเสมอ
ดังนั้นไม่มีสิ่งใดสอดคล้องกับการแยกของ CL ระหว่าง "เวกเตอร์ที่มี fill pointer" กับ "simple
vector"

## 8. ฟังก์ชันอันดับสูง

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `identity` | `(identity x)` | `T→T` | คืนอาร์กิวเมนต์ของมัน |
| `const` | `(const x y)` | `(A,B)→A` | คืนอาร์กิวเมนต์แรก |
| `compose` | `(compose f g)` | `((fn (B) C),(fn (A) B))→(fn (A) C)` | การประกอบฟังก์ชัน `f∘g` |
| `flip` | `(flip f)` | `(fn (A B) C)→(fn (B A) C)` | สลับอาร์กิวเมนต์ของฟังก์ชันสองอาร์กิวเมนต์ |
| `complement` | `(complement pred)` | `(fn (A) bool)→(fn (A) bool)` | นิเสธของ predicate |

ไม่มี `constantly` ของ CL (ชนิดของอาร์กิวเมนต์ที่ถูกละเลยจะปรากฏเฉพาะในชนิดที่คืน
และกำหนดไม่ได้) ให้เขียน `(lambda ((x T)) A v)`
