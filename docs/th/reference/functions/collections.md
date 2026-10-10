<!-- translated-from: docs/ja/reference/functions/collections.md @ 0f8a35599b7916036b773d6cb777b7de28762078 -->
# สตริง อักขระ และคอลเลกชัน

`string`, `char`, `Vector<T>`, `HashTable<K,V>`, `Array<T>`, `BitVector`, `HashSet<T>`, `SortedTable<K,V>` และ `Deque<T>`

## 1. สตริง (`string`)

สตริงเปลี่ยนแปลงไม่ได้

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `upcase` | `(upcase s)` | `string→string` | แปลงเป็นตัวพิมพ์ใหญ่ (เฉพาะ ASCII) เหมือน `string-upcase` ของ CL คืนสตริงใหม่ สตริงเปลี่ยนแปลงไม่ได้ จึงไม่มี `nstring-upcase` แบบทำลาย ฟังก์ชันนี้ใช้แทน |
| `downcase` | `(downcase s)` | `string→string` | แปลงเป็นตัวพิมพ์เล็ก (เฉพาะ ASCII) ใช้แทน `nstring-downcase` |
| `capitalize` | `(capitalize s)` | `string→string` | ทำตัวอักษรแรกของแต่ละคำเป็นตัวพิมพ์ใหญ่และที่เหลือเป็นตัวพิมพ์เล็ก (`string-capitalize` ของ CL) คำคือชุดต่อเนื่องสูงสุดของตัวอักษรและตัวเลข |
| `length` | `(length s)` | `string→int` | จำนวนอักขระ |
| `ref` | `(ref s i)` | `(string,int)→char` | อักขระตัวที่ `i` panic เมื่อเกินช่วง |
| `substring` | `(substring s start end)` | `(string,int,int)→string` | สตริงย่อย `[start,end)` |
| `append` | `(append s1 s2 ...)` | `(string,string,...)→string` | การต่อสตริง ให้ได้ตั้งแต่สามตัวขึ้นไป (เหมือน `(concatenate 'string ...)`) |
| `<` `<=` `>` `>=` | `(op s1 s2)` | `(string,string)→bool` | การเปรียบเทียบตามลำดับพจนานุกรม |
| `lt` | `(lt s1 s2)` | `(string,string)→bool` | น้อยกว่าอย่างเคร่งครัดตามลำดับพจนานุกรม (เหมือน `<`) |
| `eq` `eql` | `(op s1 s2)` | `(string,string)→bool` | การเปรียบเทียบเอกลักษณ์ (เป็นออบเจ็กต์เดียวกันหรือไม่ ไม่ใช่เนื้อหาเหมือนกัน) |
| `equal` | `(equal s1 s2)` | `(string,string)→bool` | เปรียบเทียบเนื้อหา (แยกตัวพิมพ์ใหญ่เล็ก) |
| `equalp` | `(equalp s1 s2)` | `(string,string)→bool` | เปรียบเทียบเนื้อหา (ไม่แยกตัวพิมพ์ใหญ่เล็ก เฉพาะ ASCII) |
| `/=` | `(/= s1 s2)` | `(string,string)→bool` | เนื้อหาต่างกันหรือไม่ (`string/=` ของ CL รูปแบบ variadic เปรียบเทียบคู่ที่อยู่ติดกัน) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op s1 s2)` | `(string,string)→bool` | การเรียงลำดับแบบไม่แยกตัวพิมพ์ใหญ่เล็ก (`string-lessp` ของ CL และอื่น ๆ) เมื่อมี prefix ร่วมกัน ตัวที่สั้นกว่าถือว่าน้อยกว่า |
| `string::filled` | `(string::filled n c)` | `(int,char)→string` | สตริงที่ประกอบด้วย `c` จำนวน `n` ตัว (`make-string` ของ CL) |
| `search` | `(search s sub)` | `(string,string)→Option<int>` | ตำแหน่งที่ `sub` ปรากฏครั้งแรก **`search` ของ CL มีอาร์กิวเมนต์สลับกัน** (`(search pattern sequence)`) สตริงว่างพบที่ 0 สำหรับคีย์เวิร์ด ดู[อาร์กิวเมนต์คีย์เวิร์ดของลำดับ](sequences.md#6-อาร์กิวเมนต์คีย์เวิร์ด) |
| `mismatch` | `(mismatch a b)` | `(string,string)→Option<int>` | ตำแหน่งแรกที่ต่างกัน `none` เฉพาะเมื่อ `equal` หากตัวหนึ่งเป็น prefix ของอีกตัว จะเป็นปลายของตัวที่สั้นกว่า คีย์เวิร์ดเหมือนข้างบน |
| `trim` `left-trim` `right-trim` | `(trim s)` / `(trim s bag)` | `(string &optional string)→string` | ตัดอักขระที่อยู่ใน `bag` ออกจากทั้งสองปลาย / ซ้าย / ขวา (`string-trim` ของ CL และอื่น ๆ) หากไม่มี `bag` จะเป็นช่องว่าง `" \t\n\r"` |
| `split` | `(split s sep)` | `(string,string)→Vector<string>` | แยกที่ `sep` CL ไม่มีสิ่งที่ตรงกัน ตัวคั่นที่ต่อเนื่องกันทำให้เกิดสมาชิกว่าง panic หาก `sep` ว่าง |
| `to-string` | `(to-string x)` | `T→string` | แปลงเป็นสตริงอย่างที่ `~a` ทำ implement สำหรับ `int`/`i32`/`f64`/`bool`/`char`/`string` (`princ-to-string` ของ CL) |
| `string->utf8` | `(string->utf8 s)` | `string→Vector<int>` | เข้ารหัสเป็น UTF-8 (แต่ละสมาชิก 0..255) |
| `utf8->string` | `(utf8->string bytes)` | `Vector<int>→Option<string>` | ถอดรหัส `none` หากไม่ใช่ UTF-8 ที่ถูกต้อง |

## 2. อักขระ (`char`)

`char` คือ Unicode scalar value การแปลงตัวพิมพ์และการจำแนกประเภทจัดการเฉพาะช่วง ASCII

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `upcase` | `(upcase c)` | `char→char` | แปลงเป็นตัวพิมพ์ใหญ่ (เฉพาะ ASCII) |
| `downcase` | `(downcase c)` | `char→char` | แปลงเป็นตัวพิมพ์เล็ก (เฉพาะ ASCII) |
| `<` `<=` `>` `>=` | `(op c1 c2)` | `(char,char)→bool` | เปรียบเทียบตาม code point |
| `lt` | `(lt c1 c2)` | `(char,char)→bool` | น้อยกว่าอย่างเคร่งครัดตาม code point (เหมือน `<`) |
| `alphap` | `(alphap c)` | `char→bool` | เป็นตัวอักษร ASCII หรือไม่ |
| `digitp` | `(digitp c)` | `char→bool` | เป็นหลัก ASCII หรือไม่ |
| `eq` `eql` `equal` | `(op c1 c2)` | `(char,char)→bool` | เปรียบเทียบค่า |
| `equalp` | `(equalp c1 c2)` | `(char,char)→bool` | เปรียบเทียบค่าโดยไม่สนตัวพิมพ์ใหญ่เล็ก (`char-equal` ของ CL) |
| `/=` | `(/= c1 c2)` | `(char,char)→bool` | ค่าต่างกันหรือไม่ (`char/=` ของ CL **รูปแบบ variadic เปรียบเทียบคู่ที่อยู่ติดกัน** ต่างจาก CL ที่ถามว่าทุกคู่ต่างกันหรือไม่) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op c1 c2)` | `(char,char)→bool` | การเรียงลำดับแบบไม่แยกตัวพิมพ์ใหญ่เล็ก (`char-lessp` ของ CL และอื่น ๆ) |
| `upper-casep` `lower-casep` `both-casep` | `(op c)` | `char→bool` | ตัวพิมพ์ใหญ่ / ตัวพิมพ์เล็ก / มีการแยกตัวพิมพ์ใหญ่เล็กหรือไม่ (`upper-case-p` ของ CL และอื่น ๆ) |
| `alphanumericp` | `(alphanumericp c)` | `char→bool` | ตัวอักษรหรือหลัก (ชื่อเดียวกับใน CL) |
| `graphicp` | `(graphicp c)` | `char→bool` | พิมพ์ได้หรือไม่ รวมช่องว่าง ไม่รวมการขึ้นบรรทัดใหม่หรือแท็บ (`graphic-char-p` ของ CL) |
| `standardp` | `(standardp c)` | `char→bool` | เป็นหนึ่งในอักขระมาตรฐาน 96 ตัวของ CL หรือไม่ คือ `graphicp` บวกการขึ้นบรรทัดใหม่ (`standard-char-p` ของ CL) |
| `char->int` | `(char->int c)` | `char→int` | Unicode scalar value (ทิศตรงข้ามคือ `int->char`/`try-int->char` ใน[ตัวเลข](numbers.md#1-จำนวนเต็มความกว้างคงที่)) สอดคล้องกับ `char-code`/`char-int` ของ CL |
| `char->string` | `(char->string c)` | `char→string` | สตริงหนึ่งอักขระ ฟังก์ชัน `string` ของ CL ครอบคลุมสิ่งนี้โดยรับ designator แต่ภาษานี้ไม่มี designator จึงใส่ทิศทางไว้ในชื่อ |
| `digit-weight` | `(digit-weight c)` / `(digit-weight c radix)` | `(char &optional int)→Option<int>` | **น้ำหนัก** ของหลักในฐานนั้น (`digit-char-p` ของ CL) `digitp` เป็นฟังก์ชันแยกที่คืน `bool` |
| `digit->char` | `(digit->char w)` / `(digit->char w radix)` | `(int &optional int)→Option<char>` | อักขระของน้ำหนัก `w` ตัวพิมพ์ใหญ่สำหรับ 10 ขึ้นไป (`digit-char` ของ CL; ฐานไม่เกิน 36) |
| `char->name` | `(char->name c)` | `char→Option<string>` | ชื่อของอักขระ เฉพาะอักขระที่มีชื่อซึ่งตัวอ่าน (reader) อ่านได้จึงมีชื่อ (`char-name` ของ CL) |
| `name->char` | `(name->char s)` | `string→Option<char>` | อักขระของชื่อ ไม่แยกตัวพิมพ์ใหญ่เล็ก และรับชื่อแทนของตัวอ่านด้วย (`linefeed`/`null`) (`name-char` ของ CL) |

ไม่มีค่าคงที่ที่ตรงกับ `char-code-limit` (ขอบบนของ `char` ถูกกำหนดโดย Unicode
ไม่ใช่โดยภาษา)

## 3. `Vector<T>`

อาร์เรย์ที่ขยายได้
เขียนค่าเป็น `#(1 2 3)` ได้ ([เอกสารอ้างอิงไวยากรณ์](../syntax.md#1-องค์ประกอบทางศัพท์) ชนิดของสมาชิกมาจากบริบทหรือสมาชิกตัวแรก และการประเมินค่าแต่ละครั้งสร้างเวกเตอร์ใหม่) และพิมพ์ออกมาเป็น `#(1 2 3)` เช่นกัน

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `new` | `(Vector::new)` | `()→Vector<T>` | สร้างเวกเตอร์ว่าง อาร์กิวเมนต์ชนิดมาจากชนิดที่คาดหวัง ดังนั้นใน `let` เปล่า ๆ ให้เขียน `(the Vector<i32> (Vector::new))` |
| `filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `x` จำนวน `n` ตัว |
| `push` | `(push v x)` | `(Vector<T>,T)→Unit` | ต่อท้าย |
| `get` | `(get v i)` | `(Vector<T>,int)→T` | อ่านสมาชิกตัวที่ `i` panic เมื่อเกินช่วง |
| `set` | `(set v i x)` | `(Vector<T>,int,T)→Unit` | เปลี่ยนสมาชิกตัวที่ `i` panic เมื่อเกินช่วง เขียนเป็น `(setf (get v i) x)` ได้ด้วย |
| `len` | `(len v)` | `Vector<T>→int` | จำนวนสมาชิก |
| `pop` | `(pop v)` | `Vector<T>→Option<T>` | ลบสมาชิกตัวสุดท้ายและคืนค่า `None` หากว่าง (ต่างจาก `get`/`set` ตรงที่ไม่ panic) |
| `iter` | `(iter v)` | `Vector<T>→vector-iter<T>` | สร้างตัววนซ้ำที่ implement `Iter` |
| `pushnew` | `(pushnew v x)` | `(Vector<T>,T)→Unit` where `Eq T` | ต่อท้าย `x` หากไม่มีสมาชิกที่เท่ากัน (`pushnew` ของ CL ไม่จำเป็นต้องเขียนทับ place จึงเป็นเมทอดแทนที่จะเป็นแมโคร) |

`map`/`filter` และพวกเดียวกันเป็น[ฟังก์ชันของลำดับ](sequences.md#4-ฟังก์ชันของลำดับบน-iter): ส่ง
เวกเตอร์ผ่าน `iter` เช่น `(map (iter v) f)` การดำเนินการแบบทำลาย (`nreverse`, `delete` และ
อื่น ๆ) อยู่ใน[การดำเนินการแบบทำลาย](sequences.md#7-การดำเนินการแบบทำลาย)

## 4. `HashTable<K,V>`

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `new` | `(HashTable::new)` | `()→HashTable<K,V>` | สร้างตารางว่าง |
| `get` | `(get h k)` | `(HashTable<K,V>,K)→Option<V>` | การค้นหา |
| `set` | `(set h k v)` | `(HashTable<K,V>,K,V)→Unit` | แทรกหรือเขียนทับ |
| `remove` | `(remove h k)` | `(HashTable<K,V>,K)→Option<V>` | ลบรายการและคืนค่าเดิม หากมี |
| `count` | `(count h)` | `HashTable<K,V>→int` | จำนวนรายการ |
| `clear` | `(clear h)` | `HashTable<K,V>→Unit` | ลบทั้งหมด |
| `keys` | `(keys h)` | `HashTable<K,V>→Vector<K>` | snapshot ของคีย์ |
| `values` | `(values h)` | `HashTable<K,V>→Vector<V>` | snapshot ของค่า |
| `entries` | `(entries h)` | `HashTable<K,V>→Vector<cons-cell<K,V>>` | snapshot ของคู่ `(k . v)` |
| `iter` | `(iter h)` | `HashTable<K,V>→hashtable-iter<K,V>` | ตัววนซ้ำที่ implement `Iter` สมาชิกคือ `cons-cell` `(k . v)` สอดคล้องกับ `with-hash-table-iterator` ของ CL; `doiter`/`map`/`filter` และอื่น ๆ ใช้กับมันได้ทันที |
| `maphash` | `(maphash h f)` | `(HashTable<K,V>,(fn (K V) ()))→Unit` | `maphash` ของ CL |
| `size` | `(size h)` | `HashTable<K,V>→int` | `hash-table-size` ของ CL ในตารางนี้คือจำนวนรายการที่ถูกใช้ (เท่ากับ `count`) |

**ชนิดใดก็ตามที่ implement `Hash` เป็นคีย์ได้** รวมถึงชนิด `defstruct`/`defenum` `get`/`set`/
`remove` มี `(where (Hash K))` ดังนั้นตารางที่ใช้ชนิดซึ่งไม่ได้ implement เป็นคีย์เป็น **ข้อผิดพลาด
ของชนิด** (`f64` ไม่มี `Hash` เพราะ `NaN`)

```lisp
(deftrait Hash (Eq)
  (sxhash ((self Self)) int))              ; returns a non-negative value that fits in a fixnum
```

ถูก implement สำหรับ: `int` และจำนวนเต็มความกว้างคงที่ทั้งหกชนิด, `bool`, `char`, `string` และ `symbol` (ไม่ใช่
จำนวนทศนิยม) สำหรับชนิดของคุณเอง ให้ทำให้ผลลัพธ์ไม่เป็นลบโดย `logand` กับ
`*sxhash-mask*` (2^30-1) หากต้องการแฮชสตริง เรียก `(sxhash-string s)` (FNV-1a 32 บิต) ได้ ซึ่ง
การ implement สำหรับ `string` ใช้

```lisp
(defstruct point (x int) (y int))
(impl Eq point
  (equals ((self Self) (other Self)) bool
    (if (= self::x other::x) (= self::y other::y) false)))
(impl Hash point
  (sxhash ((self Self)) int (logand (+ (* 31 self::x) self::y) *sxhash-mask*)))

(let ((h (the HashTable<point,string> (HashTable::new))))
  (progn (set h (point::new 1 2) "a")
         (get h (point::new 1 2))))          ; => (some "a")
```

คีย์สองตัวเหมือนกันหรือไม่ตัดสินโดย **ชนิดของคีย์เอง** (`sxhash` และ `equals` จาก `Eq`
ซึ่งเป็น supertrait ของ `Hash`) ไม่ใช่โดยเอกลักษณ์ของออบเจ็กต์ นั่นคือเหตุผลที่อย่างข้างบน คุณค้นหา
ด้วยคีย์ที่ "เป็นคนละค่าแต่เท่ากัน" ได้

`sxhash` ชนกันได้ไม่เป็นไร (สัญญาของ `Hash` เป็นทางเดียว: ค่าที่เท่ากันต้องมี
แฮชเดียวกัน) คีย์ที่ชนกันแยกกันด้วย `equals`

## 5. `Array<T>` (อาร์เรย์หลายมิติ)

`defstruct` ในไลบรารีมาตรฐาน ไม่ใช่ชนิดที่มีให้ในตัว ดังนั้นทุกสิ่งที่ทำกับ
`defstruct` ได้ก็ทำกับมันได้
เขียนค่าเป็น `#2A((1 2) (3 4))` ได้ ([เอกสารอ้างอิงไวยากรณ์](../syntax.md#1-องค์ประกอบทางศัพท์))

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `make` | `(Array::make dims init &key fill-pointer)` | `(Vector<int>,T)→Array<T>` | `make-array` ของ CL `dims` ถูกคัดลอก `init` คือค่าเริ่มต้นของทุกเซลล์ (`:initial-element` ของ CL; ภาษานี้ไม่มี "เซลล์ที่ไม่ได้ผูก" จึงจำเป็นต้องมี) `:fill-pointer` เฉพาะมิติเดียว |
| `get` / `set` | `(get a idx)` / `(set a idx x)` | `(Array<T>,Vector<int>)→T` | `aref` / `(setf (aref …))` ของ CL panic หากดัชนีเกินช่วง |
| `aref` | `(aref a i j …)` | — | การสะกดแบบ CL ที่ใช้ดัชนีเปล่า ขยายเป็น `get`/`set` ข้างบน `(setf (aref a i j) v)` ก็ใช้ได้ |
| `row-major-get` / `row-major-set` | `(row-major-get a i)` | `(Array<T>,int)→T` | `row-major-aref` ของ CL ดัชนีแบบแบน |
| `rank` | `(rank a)` | `Array<T>→int` | `array-rank` ของ CL |
| `dimension` | `(dimension a n)` | `(Array<T>,int)→int` | `array-dimension` ของ CL |
| `dimensions` | `(dimensions a)` | `Array<T>→Vector<int>` | `array-dimensions` ของ CL คืน **สำเนา** เช่นเดียวกับที่ CL คืนลิสต์ใหม่ |
| `total-size` | `(total-size a)` | `Array<T>→int` | `array-total-size` ของ CL (จำนวนเซลล์ที่จัดสรร ไม่เกี่ยวกับ fill pointer) |
| `len` | `(len a)` | `Array<T>→int` | `length` ของ CL บนอาร์เรย์ fill pointer หากมี มิฉะนั้นเป็น `total-size` |
| `in-bounds` | `(in-bounds a idx)` | `(Array<T>,Vector<int>)→bool` | `array-in-bounds-p` ของ CL เป็นเท็จ (ไม่ใช่ข้อผิดพลาด) แม้ **จำนวน** ดัชนีผิด |
| `row-major-index` | `(row-major-index a idx)` | `(Array<T>,Vector<int>)→int` | `array-row-major-index` ของ CL |
| `adjust` | `(adjust a dims init)` | `(Array<T>,Vector<int>,T)→Unit` | `adjust-array` ของ CL rank เปลี่ยนไม่ได้ สมาชิกที่ยังอยู่ในช่วงถูกเก็บไว้ที่ดัชนีเดิม และเซลล์ใหม่ได้ `init` ต่างจาก CL ตรงที่ไม่คืนอาร์เรย์ (ทุกอาร์เรย์ในภาษานี้ปรับได้ จึงไม่มีอาร์เรย์ตัวที่สองให้คืน) |
| `push-extend` | `(push-extend a x)` | `(Array<T>,T)→Unit` | `vector-push-extend` ของ CL panic หากไม่มี fill pointer |
| `pop` | `(pop a)` | `Array<T>→Option<T>` | `vector-pop` ของ CL `none` หากว่าง |
| `fill-pointer` | `(fill-pointer a)` | `Array<T>→Option<int>` | fill pointer (`none` หากไม่มี) เขียนได้ด้วย `(setf a::fill-pointer …)` |
| `iter` | `(iter a)` | `Array<T>→array-iter<T>` | ตัววนซ้ำตามลำดับ row-major หยุดที่ fill pointer หากมี |

- **ดัชนีเป็น `Vector<int>`** เมทอดไม่สามารถประกาศ "อาร์กิวเมนต์ชนิดเดียวกันซ้ำกี่ตัวก็ได้ที่ท้าย" และน้ำตาลไวยากรณ์
  `aref` ช่วยอุดช่องว่างนั้น
- `array-element-type` / `simple-vector-p` / `adjustable-array-p` / `array-has-fill-pointer-p`
  **ไม่มี** ชนิดสถิตของตัวรับตอบคำถามเหล่านี้อยู่แล้ว
- `Array::new` เป็นตัวสร้างตามลำดับฟิลด์ที่ `defstruct` สร้างให้ และไม่ได้มีไว้สร้าง
  อาร์เรย์ ให้ใช้ `Array::make`
- **อาร์เรย์พิมพ์ด้วยไวยากรณ์อาร์เรย์ของ CL** rank 1 คือ `#(1 2 3)`; rank อื่นคือ `#nA` ตามด้วยวงเล็บ
  ตามจำนวนระดับนั้น (`#2A((1 2 3) (4 5 6))`); rank 0 คือ `#0A5` การพิมพ์หยุดที่ fill
  pointer หากมี การตั้ง `*print-array*` ([การพิมพ์](printing.md#6-การควบคุมปริมาณที่พิมพ์))
  เป็นเท็จจะพิมพ์เพียงรูปร่าง `#<array 2x3>` เฉพาะอาร์เรย์ที่สมาชิกเป็น `defstruct`
  ที่ไม่มี `print-object` เท่านั้นที่พิมพ์ในรูปที่มีให้ในตัว `#<array<...> ...>` (ไม่ใช่ข้อผิดพลาด)

## 6. `BitVector` (เวกเตอร์ของบิต)

ลำดับบิตความยาวคงที่ `defstruct` ในไลบรารีมาตรฐาน

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `make` | `(BitVector::make n)` | `int→BitVector` | ความยาว `n` ทุกบิตเป็น 0 |
| `get` / `set` | `(get v i)` / `(set v i b)` | `(BitVector,int)→bool` | panic เมื่อเกินช่วง |
| `bit` / `sbit` | `(bit v i)` | `(BitVector,int)→bool` | การสะกดแบบ CL `(setf (bit v i) b)` ก็ใช้ได้ `sbit` ของ CL ต่างจาก `bit` เพียงตรงที่ต้องเป็น simple bit vector แต่ภาษานี้มี bit vector ชนิดเดียว |
| `len` | `(len v)` | `BitVector→int` | จำนวนบิต |
| `bit-and` `bit-ior` `bit-xor` `bit-eqv` `bit-nand` `bit-nor` `bit-andc1` `bit-andc2` `bit-orc1` `bit-orc2` | `(op a b)` | `(BitVector,BitVector)→BitVector` | คืน bit vector ใหม่ panic หากความยาวต่างกัน ไม่มีอาร์กิวเมนต์ตัวที่สามอย่างใน CL (ปลายทางของผลลัพธ์) |
| `bit-not` | `(bit-not v)` | `BitVector→BitVector` | ส่วนเติมเต็ม |

ไม่มี `bit-vector-p` (ชนิดสถิตตอบให้)

## 7. `HashSet<T>`

กลุ่มของสมาชิกที่ไม่ซ้ำกัน (`HashSet` ของ Rust) เป็น `defstruct` ของไลบรารีมาตรฐาน ข้างในคือ `HashTable<T,()>` ชนิดของสมาชิกต้อง implement `Hash` เช่นเดียวกับคีย์ของ `HashTable`

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `make` | `(HashSet::make)` | `()→HashSet<T>` | สร้างเซตว่าง อาร์กิวเมนต์ชนิดได้จากชนิดที่คาดหวัง |
| `insert` | `(insert s x)` | `(HashSet<T>,T)→bool` | เพิ่ม `x` คืน `true` ถ้าเพิ่งใส่เข้าไป `false` ถ้ามีอยู่แล้ว |
| `contains` | `(contains s x)` | `(HashSet<T>,T)→bool` | มี `x` อยู่หรือไม่ |
| `remove` | `(remove s x)` | `(HashSet<T>,T)→bool` | เอา `x` ออก คืน `true` ถ้ามีอยู่ |
| `count` | `(count s)` | `HashSet<T>→int` | จำนวนสมาชิก |
| `clear` | `(clear s)` | `HashSet<T>→Unit` | ลบทั้งหมด |
| `iter` | `(iter s)` | `HashSet<T>→vector-iter<T>` | อิเทอเรเตอร์ของสมาชิก ลำดับไม่กำหนด |

```lisp
(let ((seen (the HashSet<string> (HashSet::make))))
  (doiter (w (iter (the Vector<string> #("a" "b" "a"))))
    (if (insert seen w) () (println "dup: ~a" w))))    ; dup: a
```

สิ่งที่ทั้งสามชนิดในบทที่ 7 ถึง 9 มีร่วมกัน:

- สร้างด้วย `make` ส่วน `new` คือคอนสตรักเตอร์ตามลำดับฟิลด์ที่ `defstruct` สร้างให้ ไม่ใช่ตัวที่ใช้สร้าง (เหมือน `Array::make`)
- `iter` ไล่ดูสำเนาที่ถ่ายไว้ตอนเรียก ถ้าแก้คอลเลกชันเดียวกันในระหว่าง `doiter` ลูปนั้นจะไม่เห็น
- ถ้าชนิดของสมาชิก implement `print-object` จะพิมพ์สมาชิกในรูป `#<hashset "a" "b">` `#<sortedtable 1 "a">` `#<deque 1 2>`

## 8. `SortedTable<K,V>`

ตารางที่เรียงตามคีย์จากน้อยไปมาก (`BTreeMap` ของ Rust) ชนิดของคีย์ต้อง implement `Ord` เก็บคีย์และค่าไว้ใน `Vector` สองตัวตามลำดับคีย์ และค้นหาแบบไบนารี `set` คีย์ใหม่และ `remove` จะเลื่อนสมาชิกที่อยู่หลังตำแหน่งนั้น

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `make` | `(SortedTable::make)` | `()→SortedTable<K,V>` | สร้างตารางว่าง |
| `get` | `(get t k)` | `(SortedTable<K,V>,K)→Option<V>` | ค้นหา |
| `set` | `(set t k v)` | `(SortedTable<K,V>,K,V)→Unit` | แทรกหรือเขียนทับ |
| `remove` | `(remove t k)` | `(SortedTable<K,V>,K)→Option<V>` | ลบ และคืนค่าเก่าถ้ามี |
| `count` | `(count t)` | `SortedTable<K,V>→int` | จำนวนสมาชิก |
| `clear` | `(clear t)` | `SortedTable<K,V>→Unit` | ลบทั้งหมด |
| `keys` | `(keys t)` | `SortedTable<K,V>→Vector<K>` | คีย์ เรียงจากน้อยไปมาก |
| `values` | `(values t)` | `SortedTable<K,V>→Vector<V>` | ค่า เรียงตามคีย์ |
| `iter` | `(iter t)` | `SortedTable<K,V>→vector-iter<#{K V}>` | ทูเพิล `#{คีย์ ค่า}` เรียงตามคีย์ |

```lisp
(let ((t (the SortedTable<string,int> (SortedTable::make))))
  (set t "pear" 3) (set t "apple" 5)
  (doiter (#{k v} (iter t)) (println "~a ~a" k v)))    ; apple 5 และ pear 3
```

## 9. `Deque<T>`

ลำดับที่ใส่และเอาออกได้ทั้งสองปลาย (`VecDeque` ของ Rust)

| ชื่อ | รูปแบบ | ชนิด | คำอธิบาย |
|---|---|---|---|
| `make` | `(Deque::make)` | `()→Deque<T>` | สร้างลำดับว่าง |
| `push-front` / `push-back` | `(push-front d x)` | `(Deque<T>,T)→Unit` | เพิ่มที่หัว / ท้าย |
| `pop-front` / `pop-back` | `(pop-front d)` | `Deque<T>→Option<T>` | เอาสมาชิกที่หัว / ท้ายออกแล้วคืนให้ ถ้าว่างคืน `none` |
| `front` / `back` | `(front d)` | `Deque<T>→Option<T>` | ดูสมาชิกที่หัว / ท้าย (ไม่เอาออก) |
| `get` | `(get d i)` | `(Deque<T>,int)→Option<T>` | ตัวที่ `i` นับจากหัว ถ้าเกินช่วงคืน `none` |
| `set` | `(set d i x)` | `(Deque<T>,int,T)→Unit` | เขียนทับตัวที่ `i` ถ้าเกินช่วงจะ panic |
| `count` | `(count d)` | `Deque<T>→int` | จำนวนสมาชิก |
| `clear` | `(clear d)` | `Deque<T>→Unit` | ลบทั้งหมด |
| `iter` | `(iter d)` | `Deque<T>→vector-iter<T>` | จากหัวไปตามลำดับ |

```lisp
(let ((q (the Deque<int> (Deque::make))))
  (push-back q 1) (push-back q 2) (push-front q 0)
  (println "~s ~s ~s" (pop-front q) (pop-back q) q))    ; (some 0) (some 2) #<deque 1>
```
