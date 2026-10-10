<!-- translated-from: docs/ja/reference/functions/collections.md @ 0f8a35599b7916036b773d6cb777b7de28762078 -->
# String, Karakter, dan Koleksi

`string`, `char`, `Vector<T>`, `HashTable<K,V>`, `Array<T>`, `BitVector`, `HashSet<T>`, `SortedTable<K,V>`, dan `Deque<T>`.

## 1. String `string`

String tidak dapat diubah.

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `upcase` | `(upcase s)` | `string→string` | Mengubah menjadi huruf besar (hanya ASCII). Seperti `string-upcase` pada CL, mengembalikan string baru. String tidak dapat diubah, sehingga tidak ada `nstring-upcase` yang destruktif; fungsi ini menggantikannya |
| `downcase` | `(downcase s)` | `string→string` | Mengubah menjadi huruf kecil (hanya ASCII). Menggantikan `nstring-downcase` |
| `capitalize` | `(capitalize s)` | `string→string` | Membuat huruf pertama tiap kata menjadi huruf besar dan sisanya huruf kecil (`string-capitalize` pada CL). Kata adalah deret terpanjang berisi huruf dan angka |
| `length` | `(length s)` | `string→int` | Jumlah karakter |
| `ref` | `(ref s i)` | `(string,int)→char` | Karakter ke-`i`. Panic jika di luar rentang |
| `substring` | `(substring s start end)` | `(string,int,int)→string` | Substring `[start,end)` |
| `append` | `(append s1 s2 ...)` | `(string,string,...)→string` | Penyambungan. Tiga atau lebih dapat diberikan (sama dengan `(concatenate 'string ...)`) |
| `<` `<=` `>` `>=` | `(op s1 s2)` | `(string,string)→bool` | Perbandingan leksikografis |
| `lt` | `(lt s1 s2)` | `(string,string)→bool` | Lebih kecil dari secara leksikografis yang ketat (sama dengan `<`) |
| `eq` `eql` | `(op s1 s2)` | `(string,string)→bool` | Perbandingan identitas (apakah objeknya sama, bukan isinya sama) |
| `equal` | `(equal s1 s2)` | `(string,string)→bool` | Membandingkan isi (peka huruf besar-kecil) |
| `equalp` | `(equalp s1 s2)` | `(string,string)→bool` | Membandingkan isi (tidak peka huruf besar-kecil, hanya ASCII) |
| `/=` | `(/= s1 s2)` | `(string,string)→bool` | Apakah isinya berbeda (`string/=` pada CL. Bentuk variadik membandingkan pasangan bersebelahan) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op s1 s2)` | `(string,string)→bool` | Urutan yang tidak peka huruf besar-kecil (`string-lessp` pada CL dan sebagainya). Jika prefiksnya sama, yang lebih pendek lebih kecil |
| `string::filled` | `(string::filled n c)` | `(int,char)→string` | String berisi `n` salinan `c` (`make-string` pada CL) |
| `search` | `(search s sub)` | `(string,string)→Option<int>` | Posisi tempat `sub` pertama kali muncul. **`search` pada CL memiliki urutan argumen yang terbalik** (`(search pattern sequence)`). String kosong ditemukan di 0. Untuk keyword, lihat [argumen keyword pada sekuens](sequences.md#6-argumen-keyword) |
| `mismatch` | `(mismatch a b)` | `(string,string)→Option<int>` | Posisi pertama tempat keduanya berbeda. `none` hanya jika keduanya `equal`. Jika yang satu adalah prefiks yang lain, akhir yang lebih pendek. Keyword seperti di atas |
| `trim` `left-trim` `right-trim` | `(trim s)` / `(trim s bag)` | `(string &optional string)→string` | Menghapus karakter yang terkandung dalam `bag` dari kedua ujung / kiri / kanan (`string-trim` pada CL dan sebagainya). Tanpa `bag`, spasi putih `" \t\n\r"` |
| `split` | `(split s sep)` | `(string,string)→Vector<string>` | Memecah pada `sep`. CL tidak memiliki padanannya. Pemisah berurutan menghasilkan elemen kosong. Panic jika `sep` kosong |
| `to-string` | `(to-string x)` | `T→string` | Mengubah menjadi string seperti `~a`. Diimplementasikan untuk `int`/`i32`/`f64`/`bool`/`char`/`string` (`princ-to-string` pada CL) |
| `string->utf8` | `(string->utf8 s)` | `string→Vector<int>` | Mengodekan sebagai UTF-8 (tiap elemen 0..255) |
| `utf8->string` | `(utf8->string bytes)` | `Vector<int>→Option<string>` | Mendekode. `none` jika bukan UTF-8 yang sah |

## 2. Karakter `char`

`char` adalah nilai skalar Unicode. Konversi huruf besar-kecil dan klasifikasi hanya menangani
rentang ASCII.

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `upcase` | `(upcase c)` | `char→char` | Mengubah menjadi huruf besar (hanya ASCII) |
| `downcase` | `(downcase c)` | `char→char` | Mengubah menjadi huruf kecil (hanya ASCII) |
| `<` `<=` `>` `>=` | `(op c1 c2)` | `(char,char)→bool` | Perbandingan berdasarkan code point |
| `lt` | `(lt c1 c2)` | `(char,char)→bool` | Lebih kecil dari yang ketat berdasarkan code point (sama dengan `<`) |
| `alphap` | `(alphap c)` | `char→bool` | Apakah huruf ASCII |
| `digitp` | `(digitp c)` | `char→bool` | Apakah angka ASCII |
| `eq` `eql` `equal` | `(op c1 c2)` | `(char,char)→bool` | Membandingkan nilai |
| `equalp` | `(equalp c1 c2)` | `(char,char)→bool` | Membandingkan nilai dengan mengabaikan huruf besar-kecil (`char-equal` pada CL) |
| `/=` | `(/= c1 c2)` | `(char,char)→bool` | Apakah nilainya berbeda (`char/=` pada CL. **Bentuk variadik membandingkan pasangan bersebelahan**, tidak seperti CL, yang menanyakan apakah semua pasangan berbeda) |
| `lessp` `greaterp` `not-lessp` `not-greaterp` | `(op c1 c2)` | `(char,char)→bool` | Urutan yang tidak peka huruf besar-kecil (`char-lessp` pada CL dan sebagainya) |
| `upper-casep` `lower-casep` `both-casep` | `(op c)` | `char→bool` | Huruf besar / huruf kecil / memiliki pembedaan huruf besar-kecil sama sekali (`upper-case-p` pada CL dan sebagainya) |
| `alphanumericp` | `(alphanumericp c)` | `char→bool` | Huruf atau angka (nama sama seperti di CL) |
| `graphicp` | `(graphicp c)` | `char→bool` | Apakah dapat dicetak. Termasuk spasi, bukan baris baru atau tab (`graphic-char-p` pada CL) |
| `standardp` | `(standardp c)` | `char→bool` | Apakah salah satu dari 96 karakter standar CL, yaitu `graphicp` ditambah baris baru (`standard-char-p` pada CL) |
| `char->int` | `(char->int c)` | `char→int` | Nilai skalar Unicode (kebalikannya adalah `int->char`/`try-int->char` di [Bilangan](numbers.md#1-bilangan-bulat-berlebar-tetap)). Sepadan dengan `char-code`/`char-int` pada CL |
| `char->string` | `(char->string c)` | `char→string` | String satu karakter. Fungsi `string` pada CL mencakup ini dengan menerima designator, tetapi bahasa ini tidak memiliki designator, sehingga arahnya ada pada nama |
| `digit-weight` | `(digit-weight c)` / `(digit-weight c radix)` | `(char &optional int)→Option<int>` | **Bobot** angka pada radix itu (`digit-char-p` pada CL). `digitp` adalah fungsi terpisah yang mengembalikan `bool` |
| `digit->char` | `(digit->char w)` / `(digit->char w radix)` | `(int &optional int)→Option<char>` | Karakter untuk bobot `w`. Huruf besar untuk 10 ke atas (`digit-char` pada CL; radix paling banyak 36) |
| `char->name` | `(char->name c)` | `char→Option<string>` | Nama karakter. Hanya karakter bernama yang dapat dibaca reader yang memiliki nama (`char-name` pada CL) |
| `name->char` | `(name->char s)` | `string→Option<char>` | Karakter untuk sebuah nama. Tidak peka huruf besar-kecil, dan juga menerima alias reader (`linefeed`/`null`) (`name-char` pada CL) |

Tidak ada konstanta yang sepadan dengan `char-code-limit` (batas atas `char` ditetapkan oleh
Unicode, bukan oleh bahasa).

## 3. `Vector<T>`

Larik yang dapat membesar.
Nilainya dapat ditulis `#(1 2 3)` ([Referensi Sintaks](../syntax.md#1-unsur-leksikal); tipe elemen
ditentukan oleh konteks atau elemen pertama, dan setiap evaluasi membuat vektor baru). Ia juga
dicetak sebagai `#(1 2 3)`.

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `new` | `(Vector::new)` | `()→Vector<T>` | Membuat vector kosong. Argumen tipe berasal dari tipe yang diharapkan, sehingga pada `let` polos tulis `(the Vector<i32> (Vector::new))` |
| `filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` salinan `x` |
| `push` | `(push v x)` | `(Vector<T>,T)→Unit` | Menambahkan di akhir |
| `get` | `(get v i)` | `(Vector<T>,int)→T` | Membaca elemen `i`. Panic jika di luar rentang |
| `set` | `(set v i x)` | `(Vector<T>,int,T)→Unit` | Mengubah elemen `i`. Panic jika di luar rentang. Dapat juga ditulis `(setf (get v i) x)` |
| `len` | `(len v)` | `Vector<T>→int` | Jumlah elemen |
| `pop` | `(pop v)` | `Vector<T>→Option<T>` | Menghapus elemen terakhir dan mengembalikannya. `None` jika kosong (tidak seperti `get`/`set`, ia tidak panic) |
| `iter` | `(iter v)` | `Vector<T>→vector-iter<T>` | Membuat iterator yang mengimplementasikan `Iter` |
| `pushnew` | `(pushnew v x)` | `(Vector<T>,T)→Unit` where `Eq T` | Menambahkan `x` jika tidak ada elemen yang sama (`pushnew` pada CL. Tidak perlu menulis ulang sebuah place, sehingga ia adalah metode dan bukan makro) |

`map`/`filter` dan sejenisnya adalah [fungsi sekuens](sequences.md#4-fungsi-sekuens-pada-iter):
serahkan vector melalui `iter`, seperti `(map (iter v) f)`. Operasi destruktif (`nreverse`, `delete`,
dan sebagainya) ada di [Operasi destruktif](sequences.md#7-operasi-destruktif).

## 4. `HashTable<K,V>`

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `new` | `(HashTable::new)` | `()→HashTable<K,V>` | Membuat tabel kosong |
| `get` | `(get h k)` | `(HashTable<K,V>,K)→Option<V>` | Pencarian |
| `set` | `(set h k v)` | `(HashTable<K,V>,K,V)→Unit` | Menyisipkan atau menimpa |
| `remove` | `(remove h k)` | `(HashTable<K,V>,K)→Option<V>` | Menghapus entri dan mengembalikan nilai lama, jika ada |
| `count` | `(count h)` | `HashTable<K,V>→int` | Jumlah entri |
| `clear` | `(clear h)` | `HashTable<K,V>→Unit` | Menghapus semuanya |
| `keys` | `(keys h)` | `HashTable<K,V>→Vector<K>` | Snapshot kunci |
| `values` | `(values h)` | `HashTable<K,V>→Vector<V>` | Snapshot nilai |
| `entries` | `(entries h)` | `HashTable<K,V>→Vector<cons-cell<K,V>>` | Snapshot pasangan `(k . v)` |
| `iter` | `(iter h)` | `HashTable<K,V>→hashtable-iter<K,V>` | Iterator yang mengimplementasikan `Iter`. Elemennya adalah `cons-cell` `(k . v)`. Sepadan dengan `with-hash-table-iterator` pada CL; `doiter`/`map`/`filter` dan lainnya bekerja padanya apa adanya |
| `maphash` | `(maphash h f)` | `(HashTable<K,V>,(fn (K V) ()))→Unit` | `maphash` pada CL |
| `size` | `(size h)` | `HashTable<K,V>→int` | `hash-table-size` pada CL. Pada tabel ini ia adalah jumlah entri yang terisi (sama dengan `count`) |

**Tipe apa pun yang mengimplementasikan `Hash` dapat menjadi kunci**, termasuk tipe `defstruct`/
`defenum`. `get`/`set`/`remove` membawa `(where (Hash K))`, sehingga tabel dengan kunci bertipe yang
tidak mengimplementasikannya adalah **kesalahan tipe** (`f64` tidak memiliki `Hash` karena `NaN`).

```lisp
(deftrait Hash (Eq)
  (sxhash ((self Self)) int))              ; returns a non-negative value that fits in a fixnum
```

Diimplementasikan untuk: `int` dan enam bilangan bulat berlebar tetap, `bool`, `char`, `string`, dan
`symbol` (tidak untuk bilangan floating-point). Untuk tipe Anda sendiri, jaga hasilnya tidak negatif
dengan `logand` terhadap `*sxhash-mask*` (2^30-1). Untuk meng-hash string, Anda dapat memanggil
`(sxhash-string s)` (FNV-1a 32 bit), yang dipakai oleh implementasi untuk `string`.

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

Apakah dua kunci sama ditentukan oleh **tipe kunci itu sendiri** (`sxhash`, dan `equals` dari `Eq`,
supertrait `Hash`), bukan oleh identitas objek. Itulah sebabnya, seperti di atas, Anda dapat mencari
dengan kunci yang "bernilai berbeda tetapi sama".

Tidak masalah jika `sxhash` bertabrakan (kontrak `Hash` hanya berlaku satu arah: nilai yang sama
harus memiliki hash yang sama). Kunci yang bertabrakan dibedakan oleh `equals`.

## 5. `Array<T>` (larik multidimensi)

`defstruct` pada pustaka standar. Ia bukan tipe bawaan, sehingga semua yang dapat dilakukan dengan
`defstruct` dapat dilakukan dengannya.
Nilainya dapat ditulis `#2A((1 2) (3 4))` ([Referensi Sintaks](../syntax.md#1-unsur-leksikal)).

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `make` | `(Array::make dims init &key fill-pointer)` | `(Vector<int>,T)→Array<T>` | `make-array` pada CL. `dims` disalin. `init` adalah nilai awal setiap sel (`:initial-element` pada CL; bahasa ini tidak memiliki "sel tak terikat", sehingga wajib). `:fill-pointer` hanya untuk satu dimensi |
| `get` / `set` | `(get a idx)` / `(set a idx x)` | `(Array<T>,Vector<int>)→T` | `aref` / `(setf (aref …))` pada CL. Panic jika indeks di luar rentang |
| `aref` | `(aref a i j …)` | — | Ejaan CL dengan indeks polos. Diekspansi menjadi `get`/`set` di atas. `(setf (aref a i j) v)` juga berfungsi |
| `row-major-get` / `row-major-set` | `(row-major-get a i)` | `(Array<T>,int)→T` | `row-major-aref` pada CL. Indeks datar |
| `rank` | `(rank a)` | `Array<T>→int` | `array-rank` pada CL |
| `dimension` | `(dimension a n)` | `(Array<T>,int)→int` | `array-dimension` pada CL |
| `dimensions` | `(dimensions a)` | `Array<T>→Vector<int>` | `array-dimensions` pada CL. Mengembalikan **salinan**, seperti CL mengembalikan list baru |
| `total-size` | `(total-size a)` | `Array<T>→int` | `array-total-size` pada CL (jumlah sel yang dialokasikan, tidak terkait dengan fill pointer) |
| `len` | `(len a)` | `Array<T>→int` | `length` pada CL untuk larik. Fill pointer jika ada, jika tidak `total-size` |
| `in-bounds` | `(in-bounds a idx)` | `(Array<T>,Vector<int>)→bool` | `array-in-bounds-p` pada CL. False (bukan kesalahan) bahkan ketika **jumlah** indeks salah |
| `row-major-index` | `(row-major-index a idx)` | `(Array<T>,Vector<int>)→int` | `array-row-major-index` pada CL |
| `adjust` | `(adjust a dims init)` | `(Array<T>,Vector<int>,T)→Unit` | `adjust-array` pada CL. Rank tidak dapat berubah. Elemen yang tetap dalam rentang dipertahankan pada indeksnya, dan sel baru mendapat `init`. Tidak seperti CL, ia tidak mengembalikan larik (setiap larik di bahasa ini dapat disesuaikan, sehingga tidak ada larik kedua untuk dikembalikan) |
| `push-extend` | `(push-extend a x)` | `(Array<T>,T)→Unit` | `vector-push-extend` pada CL. Panic tanpa fill pointer |
| `pop` | `(pop a)` | `Array<T>→Option<T>` | `vector-pop` pada CL. `none` jika kosong |
| `fill-pointer` | `(fill-pointer a)` | `Array<T>→Option<int>` | Fill pointer (`none` jika tidak ada). Dapat ditulis dengan `(setf a::fill-pointer …)` |
| `iter` | `(iter a)` | `Array<T>→array-iter<T>` | Iterator dalam urutan row-major. Berhenti pada fill pointer jika ada |

- **Indeks berupa `Vector<int>`.** Metode tidak dapat mendeklarasikan "argumen bertipe sama yang
  berulang sebanyak apa pun di akhir", dan gula sintaks `aref` menjembatani kekurangan itu.
- `array-element-type` / `simple-vector-p` / `adjustable-array-p` / `array-has-fill-pointer-p`
  **tidak ada**. Tipe statis penerima sudah menjawab pertanyaan-pertanyaan itu.
- `Array::new` adalah konstruktor berurutan field yang dihasilkan `defstruct` dan tidak dimaksudkan
  untuk membuat larik. Gunakan `Array::make`.
- **Larik dicetak dalam sintaks larik CL.** Rank 1 adalah `#(1 2 3)`; rank lain adalah `#nA` diikuti
  tanda kurung sebanyak tingkat itu (`#2A((1 2 3) (4 5 6))`); rank 0 adalah `#0A5`. Pencetakan
  berhenti pada fill pointer jika ada. Menyetel `*print-array*`
  ([Pencetakan](printing.md#6-mengendalikan-seberapa-banyak-yang-dicetak)) ke false hanya mencetak
  bentuknya, `#<array 2x3>`. Hanya larik yang elemennya `defstruct` tanpa `print-object` yang
  dicetak dalam bentuk bawaan `#<array<...> ...>` (itu bukan kesalahan).

## 6. `BitVector` (vektor bit)

Urutan bit berpanjang tetap. `defstruct` pada pustaka standar.

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `make` | `(BitVector::make n)` | `int→BitVector` | Panjang `n`, semua bit 0 |
| `get` / `set` | `(get v i)` / `(set v i b)` | `(BitVector,int)→bool` | Panic jika di luar rentang |
| `bit` / `sbit` | `(bit v i)` | `(BitVector,int)→bool` | Ejaan CL. `(setf (bit v i) b)` juga berfungsi. `sbit` pada CL berbeda dari `bit` hanya dalam mensyaratkan vektor bit sederhana, tetapi bahasa ini hanya memiliki satu jenis vektor bit |
| `len` | `(len v)` | `BitVector→int` | Jumlah bit |
| `bit-and` `bit-ior` `bit-xor` `bit-eqv` `bit-nand` `bit-nor` `bit-andc1` `bit-andc2` `bit-orc1` `bit-orc2` | `(op a b)` | `(BitVector,BitVector)→BitVector` | Mengembalikan vektor bit baru. Panic jika panjangnya berbeda. Tidak ada argumen ketiga seperti di CL (tujuan hasil) |
| `bit-not` | `(bit-not v)` | `BitVector→BitVector` | Komplemen |

Tidak ada `bit-vector-p` (tipe statis menjawabnya).

## 7. `HashSet<T>`

Kumpulan elemen tanpa duplikat (`HashSet` milik Rust). Sebuah `defstruct` pustaka standar yang
isinya `HashTable<T,()>`. Tipe elemen harus mengimplementasikan `Hash`, sama seperti kunci
`HashTable`.

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `make` | `(HashSet::make)` | `()→HashSet<T>` | Membuat himpunan kosong. Argumen tipe diambil dari tipe yang diharapkan |
| `insert` | `(insert s x)` | `(HashSet<T>,T)→bool` | Menambahkan `x`. `true` bila belum ada, `false` bila sudah ada |
| `contains` | `(contains s x)` | `(HashSet<T>,T)→bool` | Apakah `x` ada |
| `remove` | `(remove s x)` | `(HashSet<T>,T)→bool` | Mengeluarkan `x`. `true` bila tadinya ada |
| `count` | `(count s)` | `HashSet<T>→int` | Jumlah elemen |
| `clear` | `(clear s)` | `HashSet<T>→Unit` | Menghapus semuanya |
| `iter` | `(iter s)` | `HashSet<T>→vector-iter<T>` | Iterator atas elemen. Urutannya tidak ditentukan |

```lisp
(let ((seen (the HashSet<string> (HashSet::make))))
  (doiter (w (iter (the Vector<string> #("a" "b" "a"))))
    (if (insert seen w) () (println "dup: ~a" w))))    ; dup: a
```

Berlaku sama untuk ketiga tipe di bab 7 sampai 9:

- Buat dengan `make`. `new` adalah konstruktor urutan field yang dihasilkan `defstruct`, bukan untuk
  membuatnya (sama seperti `Array::make`).
- `iter` menelusuri salinan yang diambil saat dipanggil. Mengubah koleksi yang sama di dalam
  `doiter` tidak terlihat oleh perulangan itu.
- Bila tipe elemen mengimplementasikan `print-object`, elemennya dicetak dalam bentuk
  `#<hashset "a" "b">` `#<sortedtable 1 "a">` `#<deque 1 2>`.

## 8. `SortedTable<K,V>`

Tabel yang terurut menaik menurut kunci (`BTreeMap` milik Rust). Tipe kunci harus
mengimplementasikan `Ord`. Kunci dan nilai disimpan dalam dua `Vector` menurut urutan kunci, dan
pencarian dilakukan secara biner. `set` untuk kunci baru dan `remove` menggeser elemen setelah
posisinya.

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `make` | `(SortedTable::make)` | `()→SortedTable<K,V>` | Membuat tabel kosong |
| `get` | `(get t k)` | `(SortedTable<K,V>,K)→Option<V>` | Pencarian |
| `set` | `(set t k v)` | `(SortedTable<K,V>,K,V)→Unit` | Menyisipkan atau menimpa |
| `remove` | `(remove t k)` | `(SortedTable<K,V>,K)→Option<V>` | Menghapus, mengembalikan nilai lama bila ada |
| `count` | `(count t)` | `SortedTable<K,V>→int` | Jumlah elemen |
| `clear` | `(clear t)` | `SortedTable<K,V>→Unit` | Menghapus semuanya |
| `keys` | `(keys t)` | `SortedTable<K,V>→Vector<K>` | Kunci-kunci, dari yang terkecil |
| `values` | `(values t)` | `SortedTable<K,V>→Vector<V>` | Nilai-nilai, menurut urutan kunci |
| `iter` | `(iter t)` | `SortedTable<K,V>→vector-iter<#{K V}>` | Tuple `#{kunci nilai}` menurut urutan kunci |

```lisp
(let ((t (the SortedTable<string,int> (SortedTable::make))))
  (set t "pear" 3) (set t "apple" 5)
  (doiter (#{k v} (iter t)) (println "~a ~a" k v)))    ; apple 5 dan pear 3
```

## 9. `Deque<T>`

Barisan yang bisa ditambah dan diambil dari kedua ujungnya (`VecDeque` milik Rust).

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `make` | `(Deque::make)` | `()→Deque<T>` | Membuat barisan kosong |
| `push-front` / `push-back` | `(push-front d x)` | `(Deque<T>,T)→Unit` | Menambah di depan / belakang |
| `pop-front` / `pop-back` | `(pop-front d)` | `Deque<T>→Option<T>` | Mengambil elemen depan / belakang dan mengembalikannya. `none` bila kosong |
| `front` / `back` | `(front d)` | `Deque<T>→Option<T>` | Melihat elemen depan / belakang (tanpa mengambilnya) |
| `get` | `(get d i)` | `(Deque<T>,int)→Option<T>` | Elemen ke-`i` dari depan. `none` bila di luar jangkauan |
| `set` | `(set d i x)` | `(Deque<T>,int,T)→Unit` | Menimpa elemen ke-`i`. Panic bila di luar jangkauan |
| `count` | `(count d)` | `Deque<T>→int` | Jumlah elemen |
| `clear` | `(clear d)` | `Deque<T>→Unit` | Menghapus semuanya |
| `iter` | `(iter d)` | `Deque<T>→vector-iter<T>` | Dari depan, berurutan |

```lisp
(let ((q (the Deque<int> (Deque::make))))
  (push-back q 1) (push-back q 2) (push-front q 0)
  (println "~s ~s ~s" (pop-front q) (pop-back q) q))    ; (some 0) (some 2) #<deque 1>
```
