<!-- translated-from: docs/ja/reference/functions/sequences.md @ eae672c1a271f0b6947f024e81dee8338b2f5ff4 -->
# Pasangan, S-Expression, dan Sekuens

Pasangan generik `cons-cell`, data S-expression `Sexpr`, simbol, fungsi sekuens yang ditulis di atas
`Iter`, dan fungsi tingkat tinggi.

## 1. Pasangan `cons-cell<A,B>`

`cons`/`car`/`cdr` adalah konstruktor dan pengakses field dari **tipe pasangan generik
`cons-cell<A,B>`** (`defstruct` pada pustaka standar). Field dapat dibaca sebagai
`variable::car`/`variable::cdr` (sintaks pengakses `defstruct` pada
[Referensi Sintaks](../syntax.md#36-defstruct--struct-tipe-buatan-pengguna)) atau sebagai
`(car variable)`/`(cdr variable)`. Untuk mengubahnya, gunakan `(setf variable::car v)`/
`(setf variable::cdr v)`.

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `cons` | `(cons a b)` | `(A,B)→cons-cell<A,B>` | Membuat pasangan |
| `car` | `(car p)` | `cons-cell<A,B>→A` | Elemen pertama |
| `cdr` | `(cdr p)` | `cons-cell<A,B>→B` | Sisanya |

`cons-cell` juga berfungsi sebagai pengganti sintaks tuple. Fungsi CL yang mengembalikan nilai ganda
(hasil bagi dan sisa pada `floor`, nilai dan posisi pada `read-from-string`, dan sebagainya)
mengembalikan `cons-cell` di bahasa ini.

## 2. Data S-expression `Sexpr`

Tipe data `Sexpr` yang dikembalikan `read` memiliki 19 varian:
`int | i8 | i16 | i32 | u8 | u16 | u32 | f32 | f64 | char | bool | sym | str | cons | ratio | path | vector | array | tuple`.
`vector` dan `array` adalah data yang ditulis sebagai `#(..)` dan `#nA(..)` ([Referensi
Sintaks](../syntax.md#1-unsur-leksikal)), yang masing-masing memuat `Vector<Option<Sexpr>>` dan
`Array<Option<Sexpr>>`: `len`, `get`, dan lainnya langsung berlaku pada `v` yang diikat oleh
`(vector v)`.
`tuple` adalah data yang ditulis dengan `#{..}`, dan `v` yang diikat `(tuple v)` adalah
`Vector<Option<Sexpr>>` baru berisi elemen-elemennya (agar tuple berapa pun panjangnya diterima
dengan satu tipe).
Sel S-expression ditangani bukan oleh `cons`/`car`/`cdr` umum pada bab 1 melainkan oleh fungsi
`sexpr-*`. Fungsi-fungsi ini dipakai terutama di badan `defmacro` untuk menyusun dan membongkar
bentuk.

**Tipe data S-expression adalah `Option<Sexpr>`.** Daftar kosong bukan varian `Sexpr` melainkan
`none` pada `Option`, dan `Sexpr` sendiri berarti "S-expression tak kosong". Jadi fungsi `sexpr-*`
menerima dan mengembalikan `Option<Sexpr>`.

- `()` adalah daftar kosong di tempat `Option<Sexpr>` diharapkan (dapat juga ditulis
  `(Option::none)`)
- `Sexpr` melebar secara implisit di tempat `Option<Sexpr>` diharapkan (tanpa konversi saat
  dijalankan). Arah sebaliknya, memakai `Option<Sexpr>` sebagai `Sexpr`, mengklaim "ini bukan daftar
  kosong", sehingga harus dinyatakan secara eksplisit dengan `match` atau `unwrap`
- Pada `match`, 19 varian `Sexpr` dan `none` dapat ditulis **rata dalam daftar cabang yang sama**
  ([Referensi Sintaks](../syntax.md#43-match--pencocokan-pola))

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `sexpr-cons` | `(sexpr-cons a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | Membuat sel `Sexpr` |
| `sexpr-car` | `(sexpr-car s)` | `Option<Sexpr>→Option<Sexpr>` | Elemen pertama. **Daftar kosong untuk daftar kosong** (seperti di CL). Panic pada atom yang bukan `Cons` |
| `sexpr-cdr` | `(sexpr-cdr s)` | `Option<Sexpr>→Option<Sexpr>` | Sisanya. **Daftar kosong untuk daftar kosong** (seperti di CL). Panic pada atom yang bukan `Cons` |
| `sexpr-consp` | `(sexpr-consp s)` | `Option<Sexpr>→bool` | Apakah `Cons` |
| `sexpr-null` | `(sexpr-null s)` | `Option<Sexpr>→bool` | Apakah daftar kosong |
| `sexpr-atom` | `(sexpr-atom s)` | `Option<Sexpr>→bool` | Apakah bukan `Cons` |
| `sexpr-symp` | `(sexpr-symp s)` | `Option<Sexpr>→bool` | Apakah `Sym` (simbol) |
| `sexpr-int` | `(sexpr-int s)` | `Option<Sexpr>→int` | Isi varian `int` (fixnum atau bignum). Panic pada tipe lain |
| `sexpr-i8` `sexpr-i16` `sexpr-i32` `sexpr-u8` `sexpr-u16` `sexpr-u32` | `(sexpr-i32 s)` | `Option<Sexpr>→W` | Isi varian dengan lebar itu. Panic pada tipe lain |
| `sexpr-f32` `sexpr-f64` | `(sexpr-f64 s)` | `Option<Sexpr>→f32` / `→f64` | Isi varian floating-point. Panic pada tipe lain |
| `sexpr-char` | `(sexpr-char s)` | `Option<Sexpr>→char` | Isi `Char`. Panic pada tipe lain |
| `sexpr-bool` | `(sexpr-bool s)` | `Option<Sexpr>→bool` | Isi `Bool`. Panic pada tipe lain |
| `sexpr-str` | `(sexpr-str s)` | `Option<Sexpr>→string` | Isi `Str`. Panic pada tipe lain |
| `sexpr-sym-name` | `(sexpr-sym-name s)` | `Option<Sexpr>→string` | Nama `Sym`. Panic pada tipe lain |
| `eq` `eql` | `(op a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Perbandingan identitas (`Cons`/`Str` membandingkan identitas objek, sisanya membandingkan nilai) |
| `equal` | `(equal a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Kesamaan struktural (`Cons` secara rekursif, `Str` berdasarkan isi) |
| `equalp` | `(equalp a b)` | `(Option<Sexpr>,Option<Sexpr>)→bool` | Seperti `equal`, ditambah perbandingan tak peka huruf besar-kecil dan perbandingan bilangan lintas tipe |
| `sexpr-append` | `(sexpr-append a b)` | `(Option<Sexpr>,Option<Sexpr>)→Option<Sexpr>` | Menyambung dua daftar `Sexpr` (tidak destruktif). `,@` diekspansi menjadi ini |
| `sexpr-map` | `(sexpr-map f lst)` | `((fn (Option<Sexpr>) Option<Sexpr>),Option<Sexpr>)→Option<Sexpr>` | Daftar `Sexpr` baru dengan `f` diterapkan pada tiap elemen daftar `Sexpr` (`map` pada bab 4 untuk `Iter` dan tidak dapat menelusuri daftar `Sexpr`) |

Ada sembilan pengakses numerik, satu per tipe, karena `Sexpr` adalah "satu-satunya tempat tipe sebuah
nilai tidak ditulis di tempat lain". `u8` yang dimasukkan ke `Sexpr` masuk sebagai varian `u8` dan
keluar hanya dengan `(sexpr-u8 s)`. Memberikannya ke `(sexpr-int s)` melakukan panic; ia tidak pernah
melebarkan jawaban secara diam-diam. Bilangan bulat pada data yang dibaca (`'(1 2 3)`, argumen makro)
adalah varian `int` dan dibaca dengan `(sexpr-int s)`.

Daftar `Sexpr` tidak memiliki operasi destruktif seperti `rplaca`/`nconc`. Sel `Sexpr` tidak dapat
diubah setelah dibuat.

## 3. Simbol

`symbol` adalah tipe simbol itu sendiri. Ia dikonversi secara implisit di tempat `Sexpr`
diperlukan, tetapi tidak secara otomatis ke arah sebaliknya.

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `symbol->string` | `(symbol->string s)` | `symbol→string` | Mengambil nama simbol |
| `string->symbol` | `(string->symbol s)` | `string→symbol` | Membuat simbol dari string (meng-intern-nya) |
| `keywordp` | `(keywordp s)` | `symbol→bool` | Apakah keyword (`:name`). Titik dua adalah bagian dari nama, sehingga pengujian melihat karakter pertama ([Referensi Sintaks](../syntax.md#1-unsur-leksikal)) |

Untuk `gensym`, lihat [Makro](system.md#8-makro).

## 4. Fungsi sekuens pada `Iter`

Fungsi sekuens adalah **fungsi generik atas trait `Iter`**. Dari sebuah koleksi, dapatkan iterator
dengan `(iter coll)` dan serahkan (`Vector<T>` / `HashTable<K,V>` / `Array<T>` mendukung ini; daftar
`Sexpr` tidak mengimplementasikan `Iter`, sehingga fungsi-fungsi ini tidak berlaku padanya). **Koleksi
hasil dikembalikan sebagai `Vector` baru.** `Iter<A>` pada tabel berarti "implementasi `Iter` apa pun
yang `Item`-nya adalah `A`". Untuk menelusuri `Vector` hasil lagi, serahkan `(iter result)`.

Fungsi yang menerima predikat (sepadan dengan keluarga `-if` pada CL):

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `map` | `(map it f)` | `(Iter<A>,(fn (A) U))→Vector<U>` | Pemetaan |
| `filter` | `(filter it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Hanya elemen yang memenuhi predikat |
| `remove-if` | `(remove-if it pred)` | `(Iter<A>,(fn (A) bool))→Vector<A>` | Menghapus elemen yang memenuhi predikat |
| `find-if` | `(find-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<A>` | Elemen pertama yang memenuhi predikat |
| `position-if` | `(position-if it pred)` | `(Iter<A>,(fn (A) bool))→Option<int>` | Posisi pertama yang memenuhi predikat |
| `count-if` | `(count-if it pred)` | `(Iter<A>,(fn (A) bool))→int` | Berapa banyak yang memenuhi predikat |
| `every` | `(every it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Apakah setiap elemen memenuhi predikat |
| `any` | `(any it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Apakah ada elemen yang memenuhi predikat (sepadan dengan `some` pada CL; nama yang tidak bentrok dengan konstruktor `Some`) |
| `foldl` | `(foldl it f init)` | `(Iter<A>,(fn (B A) B),B)→B` | Lipatan kiri |
| `foldr` | `(foldr it f init)` | `(Iter<A>,(fn (A B) B),B)→B` | Lipatan kanan |
| `collect` | `(collect it)` | `Iter<A>→Vector<A>` | Mengumpulkan semua elemen yang tersisa. Dipakai untuk mengubah hasil fungsi `lazy` di bawah menjadi `Vector` |

Pengindeksan, panjang, dan pengirisan:

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `length` | `(length it)` | `Iter<A>→int` | Jumlah elemen |
| `append` | `(append a b ...)` | `(Iter<A>,Iter<A>,...)→Vector<A>` | Menyambung iterator. Tiga atau lebih dapat diberikan |
| `concatenate` | `(concatenate 'vector it ...)` / `(concatenate 'string s ...)` | `→Vector<A>` / `→string` | `concatenate` pada CL. Tipe hasil ditulis sebagai **literal simbol yang dikutip** (CL memakai type specifier saat dijalankan). `'vector` menerima satu atau lebih, `'string` nol atau lebih (`""` untuk nol). Daftar `Sexpr` tidak tercakup (gunakan `sexpr-append`) |
| `reverse` | `(reverse it)` | `Iter<A>→Vector<A>` | Pembalikan (tidak destruktif) |
| `nth` | `(nth n it)` | `(int,Iter<A>)→Option<A>` | Elemen `n` (`None` jika di luar rentang) |
| `elt` | `(elt it n)` | `(Iter<A>,int)→Option<A>` | `nth` dengan urutan argumen terbalik |
| `take` | `(take it n)` | `(Iter<A>,int)→Vector<A>` | `n` elemen pertama |
| `subseq` | `(subseq it start end)` | `(Iter<A>,int,int)→Vector<A>` | `[start,end)` (`end` dibatasi pada panjang) |
| `last` | `(last it)` | `Iter<A>→Option<A>` | **Elemen** terakhir (bukan "sel terakhir" seperti di CL) |
| `butlast` | `(butlast it)` | `Iter<A>→Vector<A>` | Semua kecuali elemen terakhir |

Fungsi yang memerlukan batas `Eq` / `Ord` (membandingkan melalui trait, bukan predikat;
[Trait Standar](traits.md#2-eq--ord-perbandingan)):

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `member` | `(member x it)` | `(A,Iter<A>)→bool` where `Eq A` | Apakah ada elemen yang sama dengan `x` (tidak seperti CL, berupa `bool`, bukan sisa daftar) |
| `find` | `(find x it)` | `(A,Iter<A>)→Option<A>` where `Eq A` | Elemen pertama yang sama dengan `x` |
| `position` | `(position x it)` | `(A,Iter<A>)→Option<int>` where `Eq A` | Posisi pertama yang sama dengan `x` |
| `count` | `(count x it)` | `(A,Iter<A>)→int` where `Eq A` | Berapa banyak elemen yang sama dengan `x` |
| `sort` | `(sort it cmp)` | `(Iter<A>,(fn (A A) bool))→Vector<A>` | `(sort sequence predicate)` pada CL. Pengurutan stabil dan tidak destruktif. `cmp` bernilai `true` ketika "argumen pertama berada tepat sebelum argumen kedua" |
| `assoc` | `(assoc k it)` | `(K,Iter<cons-cell<K,V>>)→Option<cons-cell<K,V>>` where `Eq K` | Pasangan pertama yang `car`-nya sama dengan `k`. Ambil nilainya dengan `(cdr p)` |

Fungsi-fungsi ini dan banyak fungsi pada bab 5 juga menerima argumen keyword CL `:key` / `:test` /
`:test-not` / `:start` / `:end` / `:from-end` / `:count` (bab 6).

### Iterator malas (modul `lazy`)

Fungsi-fungsi modul `lazy` tidak membangun `Vector`, **melainkan mengembalikan iterator**. Sebuah
elemen baru dihitung ketika elemen berikutnya diminta, sehingga iterator tanpa akhir (`iterate`,
`repeat`) pun dapat dipakai selama `take` atau `take-while` di hilir menghentikannya. Setiap hasil
mengimplementasikan `Iter`, jadi fungsi `lazy` dapat disusun bertingkat, dan fungsi-fungsi pada
tabel di atas menerimanya apa adanya. `collect` mengubahnya menjadi `Vector`.

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `lazy::map` | `(lazy::map it f)` | `(Iter<A>,(fn (A) U))→Iter<U>` | Menerapkan `f` pada setiap elemen |
| `lazy::filter` | `(lazy::filter it pred)` | `(Iter<A>,(fn (A) bool))→Iter<A>` | Hanya elemen yang memenuhi syarat |
| `lazy::take` | `(lazy::take it n)` | `(Iter<A>,int)→Iter<A>` | `n` elemen pertama |
| `lazy::take-while` | `(lazy::take-while it pred)` | `(Iter<A>,(fn (A) bool))→Iter<A>` | Sampai tepat sebelum elemen pertama yang tidak memenuhi syarat |
| `lazy::skip` | `(lazy::skip it n)` | `(Iter<A>,int)→Iter<A>` | Melewati `n` elemen pertama |
| `lazy::enumerate` | `(lazy::enumerate it)` | `Iter<A>→Iter<#{int A}>` | Pasangan posisi, dihitung dari 0, dan elemen |
| `lazy::zip` | `(lazy::zip a b)` | `(Iter<A>,Iter<B>)→Iter<#{A B}>` | Pasangan yang mengambil satu dari tiap sisi. Berakhir bersama yang lebih pendek |
| `lazy::chain` | `(lazy::chain a b)` | `(Iter<A>,Iter<A>)→Iter<A>` | Elemen `a`, lalu elemen `b` |
| `lazy::flat-map` | `(lazy::flat-map it f)` | `(Iter<A>,(fn (A) Iter<B>))→Iter<B>` | Mengubah setiap elemen menjadi iterator dengan `f` dan menyambungkannya berurutan |
| `lazy::iterate` | `(lazy::iterate x f)` | `(A,(fn (A) A))→Iter<A>` | `x`, `(f x)`, `(f (f x))`, ... tanpa akhir |
| `lazy::repeat` | `(lazy::repeat x)` | `A→Iter<A>` | Mengulang `x` tanpa akhir |

`Iter<U>` dan sejenisnya dalam tabel sebenarnya adalah tipe struct yang dinamai seperti fungsinya
dengan tambahan `-iter` (untuk `lazy::map`, `lazy::map-iter<I,A,U>`, dengan `I` tipe iterator
sumber). Tipenya hanya ditulis di tempat yang tidak ditentukan oleh hal lain, misalnya tipe
kembalian lambda yang diberikan ke `lazy::flat-map`.

```lisp
(collect (lazy::take (lazy::filter (lazy::iterate 1 (lambda ((n int)) int (+ n 1)))
                                   (lambda ((n int)) bool (= 0 (mod n 3))))
                     4))                                  ; => #(3 6 9 12)

(doiter (#{i s} (lazy::enumerate (iter (the Vector<string> #("a" "b")))))
  (println "~a: ~a" i s))                                 ; 0: a dan 1: b

(-> (lazy::iterate 1 (lambda ((n int)) int (* n 2)))
    (lazy::take-while (lambda ((n int)) bool (< n 100)))
    collect)                                              ; => #(1 2 4 8 16 32 64)
```

`->` adalah makro yang meneruskan sebuah nilai sebagai argumen pertama setiap bentuk berikutnya
secara berurutan ([Option dan Result](option-result.md)).

## 5. Sisa fungsi sekuens CL

Semuanya adalah fungsi generik pada `Iter`, seperti pada bab 4. Koleksi hasil dikembalikan sebagai
`Vector` baru.

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `first`…`tenth` | `(first it)` | `Iter<A>→Option<A>` | Indeks bernama pada CL |
| `rest` | `(rest it)` | `Iter<A>→Vector<A>` | Semua kecuali yang pertama (`Vector` baru, bukan ekor yang dibagi) |
| `copy-seq` | `(copy-seq it)` | `Iter<A>→Vector<A>` | Mematerialisasi iterator menjadi `Vector` (`copy-seq`/`copy-list` pada CL) |
| `revappend` | `(revappend a b)` | `(Iter<A>,Iter<A>)→Vector<A>` | `a` dibalik, diikuti `b` |
| `Vector::filled` | `(Vector::filled n x)` | `(int,T)→Vector<T>` | `n` salinan `x` (`make-list`/`make-sequence` pada CL). Seperti `Vector::new`, argumen tipe berasal dari tipe yang diharapkan, sehingga `let` polos memerlukan `the` |
| `member-if` `member-if-not` | `(member-if it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Seperti `member`, berupa **`bool`** (iterator tidak memiliki ekor untuk dikembalikan) |
| `notany` `notevery` | `(notany it pred)` | `(Iter<A>,(fn (A) bool))→bool` | Negasi `any`/`every` |
| `find-if-not` `position-if-not` `count-if-not` `remove-if-not` | `(op it pred)` | Tipe sama seperti versi positifnya | Versi dengan predikat dinegasikan |
| `remove` | `(remove x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | Menghapus berdasarkan nilai |
| `remove-duplicates` | `(remove-duplicates it)` | `Iter<A>→Vector<A>` where `Eq A` | Menghapus duplikat. Seperti di CL, **kemunculan terakhir dipertahankan** (`:from-end true` mempertahankan yang pertama) |
| `substitute` `substitute-if` | `(substitute new old it)` | `(A,A,Iter<A>)→Vector<A>` | Mengganti berdasarkan nilai / predikat |
| `assoc-if` `rassoc` `rassoc-if` | `(rassoc v it)` | pada `Iter<cons-cell<K,V>>` | Versi predikat dan sisi nilai dari `assoc` |
| `acons` | `(acons k v it)` | `(K,V,Iter<cons-cell<K,V>>)→Vector<cons-cell<K,V>>` | Menambahkan pasangan di depan |
| `pairlis` | `(pairlis ks vs)` | `(Iter<K>,Iter<V>)→Vector<cons-cell<K,V>>` | Memasangkan dua sekuens. Berhenti pada yang lebih pendek |
| `map2` | `(map2 a b f)` | `(Iter<A>,Iter<B>,(fn (A B) U))→Vector<U>` | `mapcar` pada CL atas beberapa sekuens. Berhenti pada yang lebih pendek |
| `mapc` | `(mapc it f)` | `(Iter<A>,(fn (A) ()))→()` | Pemetaan untuk efek samping |
| `mapcan` | `(mapcan it f)` | `(Iter<A>,(fn (A) Vector<U>))→Vector<U>` | Memetakan dan menyambung |
| `maplist` | `(maplist it f)` | `(Iter<A>,(fn (Vector<A>) U))→Vector<U>` | Memetakan atas **ekor** yang berturut-turut |
| `mapl` | `(mapl it f)` | `(Iter<A>,(fn (Vector<A>) ()))→()` | Memetakan atas ekor untuk efek samping (padanan `maplist` bagi `mapc`) |
| `mapcon` | `(mapcon it f)` | `(Iter<A>,(fn (Vector<A>) Vector<U>))→Vector<U>` | Memetakan atas ekor dan menyambung (padanan `maplist` bagi `mapcan`) |
| `search` | `(search it sub)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | Posisi tempat `sub` pertama kali muncul. Jika penerimanya `string`, metode `string` yang dipilih ([String](collections.md#1-string-string)) |
| `mismatch` | `(mismatch a b)` | `(Iter<A>,Iter<A>)→Option<int>` where `Eq A` | Posisi pertama tempat keduanya berbeda. `none` jika sama |
| `merge` | `(merge a b less)` | `(Iter<A>,Iter<A>,(fn (A A) bool))→Vector<A>` | Penggabungan. CL mensyaratkan masukan terurut; ini mengurutkan hasil sambungan |
| `adjoin` | `(adjoin x it)` | `(A,Iter<A>)→Vector<A>` where `Eq A` | Menambahkan `x` **di depan** jika belum ada |
| `union` `intersection` `set-difference` `set-exclusive-or` | `(op a b)` | `(Iter<A>,Iter<A>)→Vector<A>` where `Eq A` | Operasi himpunan. CL tidak menentukan urutannya; di sini stabil, **berdasarkan urutan kemunculan pertama** |
| `subsetp` | `(subsetp a b)` | `(Iter<A>,Iter<A>)→bool` where `Eq A` | Inklusi |
| `tailp` `ldiff` | `(tailp tail whole)` | `(Iter<A>,Iter<A>)→bool` / `→Vector<A>` | Apakah sufiks / bagian sebelum sufiks. CL menanyakan **struktur yang dibagi**, tetapi tidak ada struktur untuk dibagi, sehingga ini menanyakan sufiks **sebagai nilai** |
| `seq-equals` | `(seq-equals a b)` | `(Vector<A>,Vector<A>)→bool` where `Eq A` | Kesamaan per elemen. `Vector<T>` sendiri tidak mengimplementasikan `Eq` |
| `caar`…`cddddr` | `(cadr p)` | pada pasangan bersarang | 28 fungsi CL. Menelusuri **pasangan, bukan daftar**: `cadr` menerima `cons-cell<A,cons-cell<B,C>>` |

Yang dimiliki CL dan tidak dimiliki bahasa ini: `list*` (tidak ada gagasan daftar tak patut yang
ekornya diganti), `copy-tree`/`copy-alist`/`sublis`/`subst`/`subst-if` (tidak ada tipe yang dapat
menggambarkan penelusuran pohon heterogen berkedalaman sembarang; untuk pohon `Sexpr`, `equal`
sepadan dengan `tree-equal`), keluarga property list `getf`/`get-properties`/`symbol-plist`/
`remprop` (tidak ada representasi sebagai daftar tak bertipe yang berselang-seling antara kunci dan
nilai; `assoc` (daftar asosiasi) atau `HashTable` mengisi peran yang sama), dan fungsi yang
mengonversi antara `Vector<T>` dan daftar `Sexpr` (elemen daftar `Sexpr` dapat memiliki tipe yang
berbeda-beda, sehingga tidak dapat ditulis dengan satu tipe elemen `T`).

## 6. Argumen keyword

Fungsi pada bab 4 dan 5 menerima keyword sekuens CL `:key` / `:test` / `:test-not` / `:start` /
`:end` / `:from-end` / `:count`. Semuanya **opsional**.

| Keyword | Tipe | Arti |
|---|---|---|
| `:key` | `(fn (A) A)` | Proyeksi yang diterapkan pada tiap elemen sebelum membandingkan atau menguji |
| `:test` | `(fn (A A) bool)` | Uji kesamaan yang dipakai sebagai ganti `equals` dari batas `Eq`. Argumen pertama adalah **item yang dicari**, yang kedua adalah elemen (setelah `:key`), dalam urutan yang sama seperti CL |
| `:test-not` | `(fn (A A) bool)` | Negasi `:test` |
| `:start` `:end` | `int` | Jendela `[start, end)` yang dipindai. Indeks relatif terhadap seluruh sekuens |
| `:from-end` | `bool` | Pencarian menjawab dengan kecocokan **terakhir**. Dikombinasikan dengan `:count`, elemen yang terpengaruh diambil dari akhir |
| `:count` | `int` | Jumlah maksimum elemen yang dipengaruhi keluarga `remove` / `substitute` |

Fungsi mana yang menerima keyword mana mengikuti CL:

| Fungsi | Keyword yang diterima |
|---|---|
| `find` `position` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `count` | `:key` `:test` `:test-not` `:start` `:end` |
| `member` `adjoin` | `:key` `:test` `:test-not` |
| `remove` `substitute` | Semua di atas (termasuk `:count`) |
| `remove-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `find-if` `find-if-not` `position-if` `position-if-not` | `:key` `:start` `:end` `:from-end` |
| `count-if` `count-if-not` | `:key` `:start` `:end` |
| `member-if` `member-if-not` | `:key` |
| `remove-if` `remove-if-not` `substitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `assoc` `rassoc` | `:key` `:test` `:test-not` (`:key` pada `assoc` berlaku untuk `car`, pada `rassoc` untuk `cdr`) |
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

**Perbedaan dari CL**:

1. **Proyeksi `:key` tetap dalam tipe elemen** (`(fn (A) A)`). Ia tidak dapat memproyeksikan ke tipe
   lain seperti di CL: variabel tipe tambahan tidak dapat ditentukan ketika argumen dihilangkan.
   Jika diperlukan proyeksi ke tipe berbeda, serahkan lambda ke keluarga `-if` sebagai gantinya
   (`(find-if it (lambda ((p ...)) bool (= (car p) 3)))`).
2. **Pada pencarian berbasis item, `:key` hanya berlaku pada elemen** (tidak pada item yang dicari).
   Ini aturan yang sama seperti `find`/`position`/`count`/`member`/`remove`/`substitute` pada CL.
   Pada operasi himpunan kedua sisi adalah elemen, sehingga berlaku untuk keduanya.
3. **Hanya keyword `search` yang dinamai dan bukan dinomori.** Di CL, `:start1`/`:end1` untuk
   **pola** dan `:start2`/`:end2` untuk sekuens yang dicari. Di bahasa ini penerima datang lebih
   dulu, sehingga nomor yang sama akan berarti sebaliknya, dan secara diam-diam pula.
   `:start`/`:end` untuk penerima dan `:sub-start`/`:sub-end` untuk pola, sehingga `:start1` yang
   terpakai tanpa sengaja menghasilkan kesalahan "unknown keyword". `mismatch` dan `replace`
   memiliki urutan argumen yang sama seperti CL, sehingga mempertahankan nomor CL.

## 7. Operasi destruktif

Metode `Vector<T>`. **Operasi ini mengubah penerima dan mengembalikan penerima itu sendiri**, sehingga
`(nreverse v)` ditulis sama seperti `reverse` dan `v` sendiri juga terbalik.

| Nama | Bentuk | Deskripsi |
|---|---|---|
| `nreverse` | `(nreverse v)` | Membalik di tempat |
| `delete` `delete-if` `delete-if-not` `delete-duplicates` | `(delete v x)` | Versi di tempat dari `remove` / `remove-if` / `filter` / `remove-duplicates` |
| `nsubstitute` `nsubstitute-if` | `(nsubstitute v new old)` | Versi di tempat dari keluarga `substitute` |
| `nbutlast` | `(nbutlast v)` | Membuang elemen terakhir |
| `fill` | `(fill v x)` | Menyetel setiap elemen ke `x`. Panjang tidak berubah |
| `replace` | `(replace v src)` | Menimpa dari depan dengan elemen `src`. `(min (len v) (len src))` elemen |
| `map-into` | `(map-into v src f)` | `v[i] = (f src[i])`. Jumlah yang sama seperti di atas |
| `nconc` | `(nconc v w)` | Menambahkan elemen `w` ke `v`. Tidak seperti CL, **ia tidak menulis ulang struktur yang dibagi** (`w` tidak terpengaruh) |
| `nreconc` | `(nreconc v w)` | `(nconc (nreverse v) w)` |
| `set-contents` | `(set-contents v src)` | Mengganti isi `v` dengan `src` (panjang juga berubah) |
| `rplaca` `rplacd` | `(rplaca p x)` | Menulis ulang `car`/`cdr` sebuah `cons-cell` dan mengembalikan sel itu sendiri |

Keyword yang diterima:

| Versi destruktif | Keyword yang diterima |
|---|---|
| `delete` `nsubstitute` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` `:count` |
| `delete-if` `delete-if-not` `nsubstitute-if` | `:key` `:start` `:end` `:from-end` `:count` |
| `delete-duplicates` | `:key` `:test` `:test-not` `:start` `:end` `:from-end` |
| `fill` | `:start` `:end` |
| `replace` | `:start1` `:end1` `:start2` `:end2` (penerima adalah `sequence-1` pada CL) |

`vector-push-extend`/`vector-pop` hanyalah `push`/`pop` pada `Vector<T>`. `Vector<T>` selalu
membesar, sehingga tidak ada yang sepadan dengan pembedaan CL antara "vector dengan fill pointer" dan
"vector sederhana".

## 8. Fungsi tingkat tinggi

| Nama | Bentuk | Tipe | Deskripsi |
|---|---|---|---|
| `identity` | `(identity x)` | `T→T` | Mengembalikan argumennya |
| `const` | `(const x y)` | `(A,B)→A` | Mengembalikan argumen pertama |
| `compose` | `(compose f g)` | `((fn (B) C),(fn (A) B))→(fn (A) C)` | Komposisi fungsi `f∘g` |
| `flip` | `(flip f)` | `(fn (A B) C)→(fn (B A) C)` | Menukar argumen fungsi dua argumen |
| `complement` | `(complement pred)` | `(fn (A) bool)→(fn (A) bool)` | Negasi predikat |

Tidak ada `constantly` pada CL (tipe argumen yang diabaikan hanya muncul pada tipe kembalian dan tidak
dapat ditentukan). Tulis `(lambda ((x T)) A v)`.
