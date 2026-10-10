<!-- translated-from: docs/ja/reference/syntax.md @ 276c33b026879c1ec189713e9f4318c70de2242a -->
# Referensi Sintaks typelisp

typelisp adalah Lisp dengan tipe statis, ditulis dalam S-expression. Untuk daftar fungsi bawaan dan
metode, lihat [Fungsi Bawaan](functions/README.md); untuk daftar tipe, [types.md](types.md); dan
untuk membaca pesan kesalahan, [errors.md](errors.md).

## 1. Unsur leksikal

- **Tidak peka huruf besar-kecil.** Simbol semuanya dinormalisasi menjadi huruf kecil saat dibaca.
- **Komentar**: dari `;` sampai akhir baris (komentar baris). `#| ... |#` (komentar blok, yang dapat
  bersarang).
- **Evaluasi saat pembacaan**: `#.(expr)` **menjalankan bentuk yang menyusul saat membaca** dan
  memperlakukan nilainya sebagai apa yang dibaca. Ini satu-satunya tempat reader lebih dari sekadar
  fungsi dari teks. Seberapa jauh jangkauannya bergantung pada jalur pembacaan, seperti di CL:
  - `(load ...)` dan REPL mengevaluasi satu bentuk sekali waktu, sehingga ia dapat memanggil **fungsi
    yang didefinisikan lebih awal pada teks yang sama** (`load` pada CL).
  - Berkas modul diperiksa sebagai satu kesatuan dan dijalankan oleh yang meng-`use`-nya, sehingga
    `#.` hanya dapat menjangkau pustaka standar dan apa yang sudah dijalankan sesi. Baik definisi
    berkas itu sendiri maupun definisi modul yang di-`use`-nya **belum dijalankan** (sama seperti
    `compile-file` pada CL memerlukan `eval-when`).
  - `read` / `read-from-string` di dalam program juga mengevaluasi `#.` (seperti di CL).
  - Menyetel `*read-eval*` (bawaan `true`) ke `false` membuat `#.` menjadi kesalahan pembacaan di
    mana-mana: sakelar agar teks yang dibaca sebagai data tidak menjalankan kode (seperti di CL). Ia
    dirujuk pada setiap `#.`, sehingga `setf` berlaku mulai dari bentuk berikutnya yang dibaca. Di
    dalam `with-standard-io-syntax` nilainya `true`.
- **Boolean**: `true` / `false`.
- **Bilangan bulat**: desimal (`42`, `-7`). Tanda `+`/`-` boleh di depan. Basis lain ditulis dengan
  sintaks radix CL `#b`/`#o`/`#x`/`#NNr` (tanda ditaruh setelah penanda: `#x-ff`). Awalan `0x` tidak
  ada di CL dan tidak diadopsi: `0xff` dibaca sebagai simbol.
  Literal bilangan bulat tanpa anotasi tipe bertipe `int` secara bawaan (presisi sembarang,
  [Bilangan](functions/numbers.md#3-bilangan-bulat-presisi-sembarang-int)), tanpa batas atas
  ukurannya.
  **Jika tipe yang diharapkan adalah tipe bilangan bulat berlebar tetap, literal mengambil tipe itu,
  dan diperiksa bahwa tipe itu dapat menampung nilainya**: `(the u8 300)` adalah kesalahan tipe (jika
  Anda ingin memotongnya, tulis `(as u8 300)`). `(the u32 4294967295)` dan `(the u32 #xFFFFFFFF)`
  dapat ditulis berkat aturan ini. Apakah nilai `int` muat dalam nilai langsung 63 bit atau menjadi
  bignum ditentukan oleh ukurannya, tanpa sintaks khusus (seperti di CL).
- **Bilangan floating-point**: yang mengandung titik desimal atau eksponen (`e`/`E`) (`1.5`,
  `3.0e10`). `f64` secara bawaan (`f32` jika itu tipe yang diharapkan).
- **Rasio**: `pembilang/penyebut` (hanya desimal, misalnya `1/3`). Disederhanakan saat dibaca,
  sebagaimana ditetapkan CL (`2/4` adalah `1/2`). Yang bernilai bilangan bulat (`4/2` dan sebagainya)
  dibaca sebagai `int`, bukan `ratio`. Penyebut nol (`1/0`) adalah kesalahan pembacaan.
- **Karakter**: `#\` diikuti satu karakter atau nama karakter. Misalnya `#\a` `#\Space` `#\Newline`
  `#\Tab` `#\Return` `#\Page` `#\Nul` (juga `#\Null`) `#\Backspace`. Nama tidak peka huruf
  besar-kecil.
- **String**: `"..."`. Escape-nya adalah `\n` `\t` `\r` `\0` `\\` `\"` (`\x` lain hanyalah `x`).
- **Simbol**: token apa pun yang mengandung huruf, angka, dan tanda (`+` `<=` `my-func` dan
  sebagainya).
  `]` dan `}` mengakhiri token, sehingga tidak dapat muncul di dalam simbol, dan menjumpainya di
  awal sebuah datum adalah galat pembacaan. `[` dan `{` boleh muncul di dalam simbol: seperti di CL,
  keduanya dibiarkan bebas agar pemrogram dapat memakainya dalam [makro
  pembaca](#11-reader-macro-readtable).
- **Keyword**: simbol yang diawali titik dua, seperti `:name` (seperti di CL). Keyword mengevaluasi
  dirinya sendiri: tidak mencari pengikatan dan nilainya adalah dirinya sendiri, dengan tipe statis
  `symbol`. Keyword bernama sama selalu objek yang sama (`(eq :foo :FOO)` benar; seperti simbol lain
  ia menjadi huruf kecil). Titik dua itu sendiri adalah bagian dari nama, sehingga
  `(symbol->string :foo)` adalah `":foo"` (typelisp tidak memiliki sistem paket, sehingga ini
  berbeda dari `symbol-name` pada CL). `:` sendirian atau yang memiliki titik dua tambahan seperti
  `:a:b` adalah kesalahan pembacaan. Ujilah dengan `keywordp`. Yang diawali `::` bukan keyword
  melainkan path absolut (di bawah). Perhatikan bahwa `:dyn` adalah keyword tercadang khusus untuk
  posisi tipe; menulisnya di tempat lain adalah kesalahan (lihat [bab 2](#2-menulis-tipe)).
- **Daftar**: `(a b c)`. Pasangan bertitik `(a . b)` juga dapat dibaca.
- **Vektor**: `#(1 2 3)` (seperti di CL). Isinya hanya literal dan tidak dievaluasi: `a` dalam
  `#(a b)` adalah simbol, bukan variabel. Tipe elemen ditentukan oleh konteks
  (`(the Vector<i32> #(1 2))`), atau oleh elemen pertama bila tidak ada konteks (`#(1 2 3)` adalah
  `Vector<int>`). Semua elemen harus bertipe sama: `#(1 "a")` adalah galat tipe, begitu pula `#()`
  tanpa elemen dan tanpa konteks. Setiap evaluasi membuat vektor baru. Di tempat yang mengharapkan
  data S-expression (`(the Option<Sexpr> #(1 x))`, `'#(..)`, hasil `read`), ia menjadi
  `Vector<Option<Sexpr>>` yang semua elemennya data: varian `vector` dari `Sexpr`.
- **Array**: `#2A((1 2) (3 4))` (seperti di CL). Angka di antara `#` dan `A` adalah rank, dan
  sejumlah itu tingkat pertama sarang list dalam isinya menjadi dimensi-dimensinya. `#0A x` adalah
  array berdimensi nol yang memuat satu elemen. List pada tingkat yang sama dengan panjang berbeda
  adalah galat pembacaan. Tipenya ditentukan seperti vektor dan berupa `Array<T>` (tanpa elemen,
  konteks harus memberikannya, seperti `(the Array<f64> #2A(()))`). Sebagai data S-expression ia
  adalah `Array<Option<Sexpr>>`: varian `array` dari `Sexpr`.
- **Daftar kosong `()`**: tergantung konteks, nilai tipe `Unit` atau `none` pada `Option<Sexpr>`.
  **`Sexpr` tidak memiliki varian daftar kosong**: `Sexpr` berarti "S-expression tak kosong", dan
  tipe data S-expression adalah `Option<Sexpr>` (lihat "Pola untuk `Option<Sexpr>`" pada
  [4.3 match](#43-match--pencocokan-pola)).
- **quote/quasiquote/unquote**:
  - `'x` → `(quote x)`
  - `` `x `` → `(quasiquote x)`
  - `,x` → `(unquote x)` (bermakna hanya di dalam quasiquote)
  - `,@x` → `(unquote-splicing x)` (disambungkan sebagai elemen daftar saat ekspansi)
- **Path `::`**: `foo::bar` dibaca sebagai path melalui modul, tipe, dan anggota (bukan sebagai satu
  nama simbol). Yang diawali `::`, seperti `::foo`, adalah path absolut dari akar. `::` di dalam
  argumen generik (`Vec<a::b>` dan sejenisnya) tidak diperlakukan sebagai pemisah path.

## 2. Menulis tipe

Pada kode sumber, tipe ditulis sebagai simbol atau daftar biasa.

- **Tipe primitif**: `int` `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `char` `string`
  `symbol`. `int` adalah tipe bilangan bulat (integer pada CL, berpindah otomatis antara nilai
  langsung 63 bit dan bignum; [Bilangan](functions/numbers.md#3-bilangan-bulat-presisi-sembarang-int)),
  dan enam tipe berlebar tetap dinamai menurut lebar dan tandanya (tidak ada tipe bilangan bulat 64
  bit; lihat [Bilangan](functions/numbers.md#1-bilangan-bulat-berlebar-tetap)).
- **Tipe rasional**: `ratio` (rasional dalam bentuk paling sederhana). Dialokasikan di heap seperti
  di CL, tanpa konversi implisit dengan `int`/`f64` dan sejenisnya (konversikan secara eksplisit
  dengan `as`/`try-as` atau metode konversi; lihat
  [Bilangan](functions/numbers.md#5-bilangan-rasional-ratio)).
- **Word mentah pada batas dengan C**: `ptr` (pointer buram), `c-long` / `c-ulong`. Hanya untuk FFI:
  menjadikannya sebuah nilai memerlukan `(unsafe ...)`, dan tempat kemunculannya terbatas
  ([3.3 defffi](#ptr--c-long--c-ulong--word-mesin-mentah)). Jangan pakai ini di tempat Anda
  menginginkan bilangan bulat 64 bit: tipe ini tidak memiliki aritmetika.
- **Tipe mutable buram**: `random-state` (keadaan generator bilangan acak). Ia tidak dapat masuk ke
  `Vector<T>`/`HashTable<K,V>`/`Sexpr` (ia dapat masuk ke `Option<T>`/`Result<T,E>`).
- **Tipe Unit**: `()`
- **Tipe Never**: `!` (tipe ekspresi yang tidak kembali seperti `panic`/`unreachable`/`todo`/
  perulangan yang tidak pernah kembali. Ia cocok dengan tipe apa pun yang diharapkan)
- **Tipe fungsi**: `(fn (tipe-argumen...) tipe-kembalian)`. Tipe fungsi dengan argumen variadik
  adalah `(fn (tipe-argumen... &rest tipe-elemen) tipe-kembalian)`.
- **Tipe generik**: `Name<T1,T2,...>` (dibaca sebagai satu token tanpa spasi). Misalnya
  `Option<i32>` `Result<i32,ParseIntError>` `HashTable<string,i32>` `Vector<T>`. Tipe unit `()`
  juga dapat ditulis sebagai argumen tipe (`Result<(), FileError>`). `(`/`)` biasanya adalah
  pembatas yang mengakhiri token, tetapi selama kurung sudut terbuka, sepasang karakter ini
  diloloskan. `()` juga dapat dipakai sebagai tipe field atau tipe argumen.
- **Bentuk penerapan tipe generik**: `(Name T1 T2 ...)`, ejaan daftar yang menamai tipe yang sama
  dengan `Name<T1,T2,...>`. Misalnya `(vector char)` sama dengan `Vector<char>`. Bentuk nama adalah
  cara penulisan yang biasa; bentuk ini **ada untuk ketika argumen tipe tidak dapat dieja di dalam
  nama**: argumen tipe sendiri adalah ekspresi tipe, tetapi di dalam nama satu token hanya nama,
  `()`, dan `:dyn` yang dapat ditulis, bukan tipe fungsi (tidak ada ejaan seperti
  `Vector<(fn (i32) i32)>`). Ia juga dapat muncul dalam bentuk ini ketika implementasi menampilkan
  tipe, seperti hasil mensubstitusi tipe terkait sebuah trait ke dalam signature.
- **Nama tipe terkualifikasi**: dapat dikualifikasi dengan `::`, seperti `module::Type`.
- **Tipe objek trait**: `:dyn Trait` (dua kata yang dipisahkan spasi membentuk satu tipe).
  Mewakili nilai yang tipe konkretnya ditentukan saat dijalankan; pemanggilan metode trait melalui
  vtable (dispatch dinamis). Untuk trait dengan tipe terkait, tipe itu ditetapkan secara posisional
  mengikuti urutan deklarasi (`:dyn Iter<i32>` menetapkan `Item` ke `i32`). Ia juga dapat ditulis di
  dalam argumen generik: `Vector<:dyn Drawable>` `HashTable<string, :dyn Drawable>`. Nilai konkret
  dikotakkan secara otomatis pada posisi yang diharapkan; bentuk eksplisitnya adalah
  `(as :dyn Trait expr)`. Nilai `:dyn Sub` dapat diserahkan apa adanya di tempat `:dyn Super` dari
  salah satu supertrait-nya (semua yang diwarisinya, secara transitif) diperlukan (upcasting). Ia
  tidak dapat diserahkan ke trait yang tidak berkaitan. Untuk syarat yang harus dipenuhi sebuah trait
  agar dapat dipakai dengan `:dyn`, lihat [3.9 deftrait / impl](#39-deftrait--impl--trait). Menulis
  `:dyn` di luar posisi tipe adalah kesalahan.
- Tipe generik bawaan: `Option<T>` (`Some(T)` / `None`), `Result<T,E>` (`Ok(T)` / `Err(E)`),
  `HashTable<K,V>`, `Vector<T>`, dan tipe konkurensi `Task<T>` / `Thread<T>` / `Chan<T>`
  ([bab 12](#12-konkurensi-task)). Ada juga `Sexpr`, tipe data S-expression. Tipe kesalahan konkret
  bawaan adalah `ParseIntError` / `ParseFloatError` / `ReadError` / `EvalError` / `FileError` /
  `NetError`, dan pustaka standar memiliki struct `SimpleError` / `WrappedError` (`Error` bukan tipe
  melainkan trait: pakai sebagai `:dyn Error`). Daftarnya ada di [types.md](types.md).
- **Tipe dan trait berbagi satu ruang nama** (seperti di Rust): dalam satu modul, sebuah tipe
  (`defstruct`/`defenum`) dan sebuah trait (`deftrait`) tidak dapat bernama sama.

## 3. Definisi tingkat atas

### 3.1 defun — definisi fungsi

```lisp
(defun name ((arg1 Type1) (arg2 Type2) ...) RetType
  body...)
```

- Tipe argumen dan tipe kembalian wajib.
- Fungsi generik menulis parameter tipenya dalam kurung sudut setelah namanya:
  `(defun name<T1,T2...> (params) Ret body...)` (sintaks kurung sudut yang sama seperti `Vector<T>`
  pada posisi tipe).
- `defun`/`lambda`/`defmethod` menerima argumen variadik ketika `&rest (name Type)` ditulis di akhir:
  `(defun name ((a Type1) &rest (xs Type2)) Ret body...)` (di badan, `xs` selalu terikat sebagai
  `Option<Sexpr>`, daftar S-expression. Setiap argumen aktual pada pemanggilan diperiksa tipenya
  sebagai `Type2` satu per satu lalu dibungkus menjadi `Sexpr`). `defmacro` juga memiliki `&rest`
  sendiri, tetapi berbeda karena selalu `Sexpr` tak bertipe (`defun`/`lambda` menyatakan tipe
  elemen). Tipe fungsi juga dapat menggambarkan fungsi variadik, sebagai `(fn (T1... &rest Te) Ret)`.
- **`&optional` / `&key`** (untuk `defun` dan `defmethod`; tidak untuk `lambda`/`labels`, karena alasan
  di bawah, dan `defmacro` memiliki implementasi terpisah, juga di bawah). Urutannya seperti CL:
  `required &optional &rest &key`. Setiap parameter ditulis `(name Type)` atau
  `(name Type default-expr)`:

  ```lisp
  (defun greet ((name string) &optional (suffix string)) string      ; no default
    (match suffix ((some s) (append name s)) ((none) name)))         ; Option<string> in the body

  (defun pow ((b i32) &optional (n i32 2)) i32 ...)                  ; with a default
  (pow 3)      ; n = 2
  (pow 3 5)    ; n = 5

  (defun mk (&key (a i32 0) (b string "z")) string ...)
  (mk :b "q")  ; the caller writes `:name value`, in any order; omitted ones take their defaults
  ```

  - **Parameter tanpa ekspresi bawaan bertipe `Option<Type>`.** Jika dihilangkan, ia `none`; jika
    diberikan, nilai polos yang ditulis pemanggil dibungkus dalam `some` secara otomatis. Apa yang
    dilakukan CL dengan variabel supplied-p ("apakah diberikan?") muncul pada sisi tipe statis.
  - Dengan ekspresi bawaan, tipenya tetap `Type` sebagaimana dideklarasikan. Ketika dihilangkan,
    **ekspresi yang sudah diperiksa** itu disematkan pada pemanggilan apa adanya (dievaluasi pada
    setiap pemanggilan).
  - **`&key` tidak dapat dicampur dengan `&optional`/`&rest` dalam satu daftar argumen.** Ini
    menghindari ambiguitas yang dimiliki CL sendiri (apakah argumen aktual di akhir diambil oleh
    `&optional` posisional atau dicocokkan dengan label sebagai `&key` bergantung pada *nilai*) dengan
    melarang kombinasinya. `&optional` dan `&rest` dapat dipakai bersama.
  - Semuanya dapat dipakai pada fungsi generik, tetapi **parameter tipe yang hanya muncul pada
    argumen yang dihilangkan tidak dapat disimpulkan dan merupakan kesalahan** (tidak ada nilai untuk
    dicocokkan).
  - **`defmethod` dapat memiliki tiga bagian yang sama** (baik untuk metode instans maupun fungsi
    statis). Daftarkan `&optional`/`&rest`/`&key` setelah penerima:

    ```lisp
    (defstruct box (w i32) (h i32))
    (defmethod grow ((self box) &key (dw i32 0) (dh i32 0)) i32 ...)
    (grow (box::new 1 2) :dh 10)

    (defmethod origin (point &key (x i32 0) (y i32 0)) point (point::new x y))   ; static function
    (point::origin :y 7)
    ```

    Semuanya dapat dipakai pada metode tipe generik juga, tetapi **tipe parameter dengan ekspresi
    bawaan tidak boleh menyebut parameter tipe milik pemilik** (batasan yang sama yang dimiliki
    `defun` untuk parameter tipenya sendiri: yang disematkan ketika argumen dihilangkan adalah
    ekspresi yang *sudah diperiksa*, sehingga tipenya tidak dapat dibiarkan sebagai variabel
    abstrak).
  - **Semuanya tidak dapat dipakai pada metode trait.** `deftrait` tidak memiliki sintaksnya, dan
    jika hanya sisi `impl` yang dapat mendeklarasikan bagian-bagian itu, pemanggilan dengan penerima
    `:dyn` (mengisi argumen dari deklarasi trait) dan pemanggilan dengan penerima konkret (mengisinya
    dari deklarasi `impl`) akan menjadi hal yang berbeda. Arity slot vtable ditetapkan.
  - **Semuanya tidak dapat dipakai pada `lambda` / `labels`** (`&rest` dapat). Untuk mengisi argumen
    yang dihilangkan, pemanggil harus membaca **ekspresi bawaan yang sudah diperiksa milik yang
    dipanggil**, yang hanya tersedia dari signature yang diselesaikan berdasarkan nama. `lambda`
    dioper sebagai nilai, dan satu-satunya yang menggambarkan nilai itu adalah tipe fungsinya
    `(fn ...)`: tidak ada tempat di dalamnya untuk sebuah ekspresi, dan seandainya ada, "dua lambda
    dengan signature sama tetapi bawaan berbeda" akan menjadi tipe yang berbeda. `&rest` tetap dalam
    urusan tipe, sehingga dapat ditulis dalam tipe fungsi.
- **Rujukan maju dideklarasikan dengan `defsignature`** (di bawah). Nama yang belum dideklarasikan
  tidak dapat dipanggil sebelum definisinya, karena tingkat atas diperiksa dan dijalankan satu bentuk
  sekali waktu, sesuai urutan sumber.
- Untuk mensyaratkan batas trait, tulis klausa `where` tepat sebelum badan:
  `(defun name<T> (params) Ret (where (Trait T (AssocName ConcreteType)...)) body...)`
  (menetapkan tipe terkait dengan `(AssocName ConcreteType)` bersifat opsional).
- **Docstring**: literal string di awal badan, tepat setelah klausa `where` (jika ada), menjadi
  docstring (seperti di CL). Hanya ketika setidaknya satu bentuk badan menyusulnya, tetapi: string
  tunggal tetap menjadi nilai kembalian dan tidak dianggap docstring:
  `(defun f () string "doc" "value")` memiliki docstring dan mengembalikan `"value"`, sedangkan
  `(defun f () string "value")` tidak memiliki docstring dan mengembalikan `"value"`. Docstring dapat
  diambil dengan `(documentation name)`
  ([docstring](functions/system.md#7-docstring--documentation)).

### 3.2 defsignature — deklarasi maju

```lisp
(defsignature name (argument-types...) return-type)
(pub defsignature name (argument-types...) return-type)
```

Untuk memanggil `defun` yang didefinisikan **setelah** diri Anda, deklarasikan lebih dulu seperti
ini. Rekursi timbal balik hanya dapat ditulis dengan cara ini:

```lisp
(defsignature odd2 (i32) bool)
(defun even2 ((n i32)) bool (if (= n 0) true  (odd2 (- n 1))))
(defun odd2  ((n i32)) bool (if (= n 0) false (even2 (- n 1))))
```

Argumen didaftarkan sebagai **tipe saja**; tidak ada badan, sehingga tidak ada yang perlu diberi
nama. `&rest` dapat ditulis di akhir, sebagai `&rest element-type`.

Deklarasi **diperiksa**:

- Definisi yang menyusul harus cocok dengan deklarasi (jumlah dan tipe argumen, tipe kembalian,
  `&rest`, dan apakah `pub`). Ketidakcocokan adalah kesalahan pada definisi.
- Mendeklarasikan tanpa mendefinisikan adalah kesalahan (dilaporkan ketika berkas / modul selesai
  dimuat). REPL tidak melaporkannya setelah tiap masukan, karena deklarasi dan definisinya harus
  dapat diketik pada baris terpisah.
- Deklarasi yang ditaruh **setelah** definisi adalah kesalahan, karena deklarasi seperti itu tidak
  dapat melakukan apa-apa.

Tiga hal tidak dapat dideklarasikan:

- **Fungsi generik.** Membuat salinan untuk setiap tipe memerlukan badan, dan deklarasi tidak
  memilikinya. Pemanggilan maju dapat diselesaikan tetapi instansiasi akan gagal, sehingga
  deklarasinya ditolak sejak awal.
- **`&optional`/`&key`.** Signature-nya mencakup ekspresi **yang sudah diperiksa** dari tiap nilai
  bawaan (disematkan pada pemanggilan ketika argumen dihilangkan), dan deklarasi tidak memiliki
  tempat untuknya.
- **Apa pun selain `defun`.** `defmacro` memerlukan badan makro **sudah dijalankan** agar dapat
  berekspansi, yang tidak dapat digantikan oleh pendaftaran signature. Untuk tipe
  (`defstruct`/`defenum`/`deftrait`), mendaftarkannya adalah "apa yang diperlukan kode yang
  mendaftarkan tipe itu sendiri", yang tidak berdiri sendiri seperti signature. `defmethod`
  didaftarkan pada tipe yang memilikinya, sehingga mengikuti tipe.

Padanan di CL adalah `(declaim (ftype (function (i32) bool) even2))`, tetapi itu datang bersama
seluruh sistem deklarasi dan hanya bersifat **anjuran**. Di sini, dengan tipe statis, deklarasi
diperiksa.

### 3.3 defffi — deklarasi fungsi C (FFI)

```lisp
(defffi (name "c_symbol") (argument-types...) return-type)
(defffi (name "c_symbol") (argument-types...) return-type :library "name")
(defffi name (argument-types...) return-type)              ; name = the C symbol name
(pub defffi ...)
```

Mendeklarasikan fungsi C agar dapat dipanggil. Bentuknya sama seperti `defsignature` (nama, tipe
argumen, tipe kembalian, dan tanpa badan), tetapi tidak adanya badan bermakna berbeda.
`defsignature` adalah janji bahwa "saya akan mendefinisikannya nanti", sedangkan `defffi`
menyatakan bahwa "orang lain sudah menulis dan mengompilasi badannya".

```lisp
(defffi (c-abs "abs") (i32) i32)
(defffi (c-sqrt "sqrt") (f64) f64)
(defffi (c-getpid "getpid") () i32)

(unsafe (c-abs -5))                          ; => 5
```

Nama typelisp dan nama simbol C dapat ditulis terpisah karena pengenal typelisp biasanya mengandung
`-` dan pengenal C tidak dapat. Jika nama C dihilangkan, nama itu dipakai sebagai nama simbol C
apa adanya.

**Pemanggilan memerlukan `(unsafe ...)`** (bahkan untuk fungsi yang hanya memakai skalar). Kompiler
tidak punya cara memastikan bahwa signature C yang dideklarasikan cocok dengan yang sebenarnya dan
hanya dapat memercayai deklarasi; `unsafe` adalah tanda bahwa Anda menanggung tanggung jawab itu.
Cara yang dimaksudkan adalah membungkusnya sekali dan membuat pembungkus yang aman:

```lisp
(defun abs-i32 ((n i32)) i32 (unsafe (c-abs n)))
(abs-i32 -3)                                 ; no unsafe needed from here on
```

Tipe yang dapat ditulis adalah `i8` `i16` `i32` `u8` `u16` `u32` `f32` `f64` `bool` `()` (void)
`string` `ptr` `c-long` `c-ulong`, dan pointer bertipe `(ptr T)`
([di bawah](#def-c-struct-dan-pointer-bertipe--mengalokasikan-struct-c)).

`string` adalah `const char *`. String typelisp tidak berakhiran NUL dan dapat mengandung NUL itu
sendiri, sehingga **string disalin menjadi string C saat dioper**, dan dibebaskan setelah
pemanggilan. NUL dalam string adalah kesalahan: C hanya akan melihat sampai situ, sehingga string
yang berbeda akan dioper secara diam-diam.

**String yang dikembalikan juga disalin**, dan tidak dibebaskan: yang dikembalikan C adalah milik C,
dan mungkin menunjuk ke tabel statis, seperti pada `getenv`. Fungsi yang mengembalikan memori yang
harus dibebaskan pemanggil (`strdup` dan sebagainya) sebaiknya diambil sebagai `ptr` dan dibebaskan
sendiri.

Fungsi yang hasilnya menunjuk ke dalam argumen (`strchr`, `strstr`) juga bekerja dengan benar: hasil
disalin sebelum argumen dibebaskan.

Jika fungsi yang dideklarasikan mengembalikan `string` ternyata mengembalikan NULL, itu adalah
kesalahan, karena `string` tidak memiliki nilai yang berarti "tidak ada". Jika NULL mungkin terjadi,
ambil hasilnya sebagai `ptr`.

```lisp
(defffi (c-strlen "strlen") (string) i32)
(defffi (c-getenv "getenv") (string) string)

(unsafe (c-strlen "hello"))                  ; => 5
(unsafe (c-getenv "PATH"))                   ; => "/usr/bin:..."
```

Dengan `:library`, pustaka bersama itu dibuka dan simbol dicari di dalamnya. Tanpanya, simbol dicari
di **proses itu sendiri** (semua yang sudah tertaut, termasuk libc). Nama pendek seperti `sqlite3`
dicari sebagai `libsqlite3.dylib` / `libsqlite3.so` dalam urutan itu, dan nama yang mengandung `/`
diperlakukan sebagai path. Pustaka yang dibuka tidak pernah ditutup: kode yang menunjuk fungsinya
terus berjalan, sehingga satu-satunya masa hidup yang benar adalah masa hidup proses.

#### ptr / c-long / c-ulong — word mesin mentah

`ptr` adalah pointer buram (`void *`, `FILE *`, apa pun maksud deklarasinya). `c-long` / `c-ulong`
adalah `long` / `unsigned long` pada C (juga `size_t`, `int64_t`, dan `intptr_t`).

```lisp
(defffi (c-malloc "malloc") (c-ulong) ptr)
(defffi (c-free "free") (ptr) ())
(defffi (c-strlen "strlen") (string) c-ulong)

(unsafe (let ((p (c-malloc 16))) (c-free p) ()))
```

**Tidak menamainya `i64` / `u64` adalah disengaja.** Bahasa ini tidak memiliki tipe bilangan bulat 64
bit, karena nilai langsung bertag hanya memiliki 63 bit ([bab 2](#2-menulis-tipe)). Nama `c-long`
mengatakan "ini adalah word yang menyeberangi batas dengan C, bukan bilangan bulat bahasa ini".

**Tipe ini tidak memiliki aritmetika.** `(+ x 1)` tidak dapat ditulis. Ia dapat disediakan tetapi
tidak, agar tidak ada komputasi yang berjalan pada nilai yang tidak dapat disimpan di mana pun dan
berlebar berbeda dari setiap bilangan lain, untuk alasan yang sama tipe bilangan bulat 64 bit
ditiadakan. Yang ada **hanya konversi**:

```lisp
(as i32 (unsafe (c-strlen "hello")))         ; read what came back
(as int (unsafe (c-strlen s)))               ; this one to read it exactly (int does not lose 64 bits)
(try-as i32 (unsafe (c-strlen s)))           ; ask whether it fits
(as c-ulong n)                               ; make one from another integer
```

**Literal** bilangan bulat mengambil tipe yang diharapkan, sehingga tidak perlu `as` hanya untuk
mengoperkannya:

```lisp
(unsafe (c-malloc 16))                       ; 16 is read as a c-ulong
```

Literal di luar rentang ditolak seperti pada lebar lain (`(c-malloc -1)` tidak muat dalam
`c-ulong`).

**Tempat kemunculannya terbatas**: hanya tipe argumen, tipe kembalian, dan variabel lokal. Masing-masing
berikut adalah kesalahan:

```lisp
(defstruct handle (p ptr))          ; a struct field
(defenum maybe (none) (some ptr))   ; an enum field
(defvar (block ptr) ...)            ; a global
(defffi f ((vector ptr)) i32)       ; inside a type argument
```

Alasannya satu untuk semuanya: **slot memberi tag pada apa yang disimpannya**. Pemberian tag akan
membuang bit teratas pointer, alasan yang sama tipe bilangan bulat 64 bit ditiadakan, sehingga tidak
diizinkan bahkan di dalam `unsafe`. Ini bukan soal izin: representasi itu tidak ada.

Karena alasan yang sama, tipe ini tidak dapat menjadi variabel lokal yang **ditangkap** oleh fungsi
bersarang (pengikatan yang ditangkap masuk ke sebuah sel, dan sel memberi tag pada apa yang
disimpannya). Ini diketahui saat kompilasi dan dilaporkan oleh `(compile f)`.

GC tidak menelusuri `ptr`. Ia menunjuk ke luar heap, sehingga itu benar.

Empat hal tidak dapat dideklarasikan:

- **Argumen variadik** (`printf`). Bagian variadik dioper dengan aturan yang berbeda dari argumen
  tetap (di stack pada AArch64 Darwin), sehingga tidak dapat dipanggil dengan benar dari signature
  tetap. `&rest` ditolak.
- **Mengoper atau mengembalikan struct secara nilai.** Karena alasan yang sama (bergantung pada
  konvensi pemanggilan tiap platform). Tipe yang dapat ditulis dibatasi pada daftar di atas,
  sehingga tidak dapat dieja.
- **Generik.** C tidak memiliki padanannya.
- **Nama yang sama dengan fungsi bawaan.** Pemanggilan terkompilasi akan menyelesaikan nama itu ke
  fungsi bawaan, sehingga ditolak daripada diam-diam salah.

#### Callback — membuat C memanggil balik

Menulis tipe fungsi `(fn (types...) return-type)` sebagai tipe argumen menjadikan argumen itu fungsi
yang dipanggil balik oleh C (callback).

```lisp
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn (ptr ptr) i32)) ())

(defun desc ((a ptr) (b ptr)) i32 ...)

(unsafe
  (c-qsort buf n 8 desc)                                 ; a top-level function
  (c-qsort buf n 8 (lambda ((a ptr) (b ptr)) i32 ...))   ; a lambda
  (labels ((cmp ((a ptr) (b ptr)) i32 ...))
    (c-qsort buf n 8 cmp)))                              ; a local function
```

Pointer fungsi C tidak lain adalah alamat kode, dan C memanggilnya dengan hanya meneruskan argumen
yang dideklarasikan. Tidak ada tempat untuk meneruskan variabel yang ditangkap, sehingga **hanya
fungsi tanpa variabel bebas yang dapat diserahkan**, dan ini diperiksa pada saat pemeriksaan tipe.

- Tulis nama fungsi atau ekspresi `lambda` **langsung** sebagai argumen aktual. Variabel yang
  menyimpan fungsi tidak dapat diserahkan: fungsi mana yang disimpannya, dan karenanya apakah ia
  memiliki variabel bebas, tidak diketahui sampai saat dijalankan.
- `lambda` adalah kesalahan jika merujuk variabel lokal di luarnya. Variabel global dan fungsi
  tingkat atas boleh dirujuk.
- Fungsi lokal (`labels`) tidak boleh memiliki variabel bebas, termasuk milik fungsi saudara yang
  dipanggilnya. Fungsi saudara berbagi tempat penyimpanan variabel yang ditangkap, sehingga apa yang
  ditangkap saudara yang dipanggil juga ditangkap oleh fungsi ini.
- Fungsi generik mendapatkan tipenya dari tipe fungsi yang dideklarasikan.
- Tipe yang dapat ditulis pada tipe fungsi sama dengan daftar di atas. Namun, `string` tidak dapat
  menjadi tipe kembalian callback (ia akan menyerahkan memori yang tidak dibebaskan siapa pun kepada
  C). Argumen `string` menyalin string yang dioper C menjadi string typelisp.

Pemanggilan fungsi C hanya dapat ditulis di dalam `unsafe`, sehingga callback hanya dapat diserahkan
di dalam `unsafe`.

**Callback hanya dapat dipanggil selama fungsi C yang dipanggil typelisp sedang berjalan.** Jika
dipanggil dari tempat lain (thread yang tidak menjalankan typelisp, penangan sinyal, fungsi yang
didaftarkan dengan `atexit`), ia mencetak alasannya dan menghentikan proses.

**Kegagalan tidak merambat melalui C.** `panic` atau `throw` di dalam callback tidak dapat melepas
melalui frame C (itu akan menjadi perilaku tak terdefinisi), sehingga 0 dikembalikan ke C, dan
kegagalan dilempar ulang ke pemanggil ketika fungsi C kembali. Jika callback dipanggil lagi di antara
kegagalan dan kembalinya fungsi C, ia tidak dijalankan dan 0 dikembalikan.

Operasi yang harus menunggu di dalam callback (`recv` pada kanal kosong dan sebagainya) adalah
kesalahan ([12.6](#126-kode-terkompilasi-dan-task)).

Ketika sebuah fungsi didefinisikan ulang, definisi baru dipanggil mulai dari saat berikutnya ia
diserahkan ke C.

Ia bekerja sama dengan AOT (`compile-file`). Titik masuk yang dipanggil C dibangun ke dalam berkas
eksekusi.

**Tidak dapat diserahkan sebagai nilai.** Deklarasi FFI tidak dapat ditulis apa adanya untuk `f`
pada `(map f xs)`: nilai fungsi adalah closure yang membungkus badan sebuah definisi, dan deklarasi
ini tidak memiliki badan untuk dibungkus. Bungkuslah dengan `lambda`:

```lisp
(run-it (unsafe (lambda ((n i32)) i32 (c-abs n))))
```

`(disassemble c-abs)` juga ditolak: yang dapat ditampilkan adalah kode mesin C, yang tidak dihasilkan
kompiler ini. `(compile c-abs)` berhasil (dan tidak melakukan apa-apa, karena sudah terkompilasi).

**Ia bekerja dengan AOT (`compile-file`) juga.** Linker menyelesaikan fungsi C itu sendiri. Jika
sebuah deklarasi memiliki `:library`, pustaka itu ditambahkan ke baris penautan sebagai `-l`
(duplikat digabung menjadi satu), sehingga `compile-file` tidak memerlukan argumen tambahan.
`compile-file` sendiri membaca sumbernya, sehingga ia dapat mengumpulkannya dari deklarasi.

Simbol dicari saat build juga. Jika fungsi yang dideklarasikan tidak ada, kesalahan menyebut namanya
sebelum kesalahan penautan apa pun.

Pustaka standar (prelude) tidak memakai `defffi`. Pustaka standar masuk ke setiap berkas eksekusi
secara utuh, sehingga deklarasi dengan `:library` di sana akan menautkan pustaka itu bahkan ke
program yang tidak memakai FFI.

#### def-c-struct dan pointer bertipe — mengalokasikan struct C

```lisp
(unsafe
  (def-c-struct name (field type)...)
  ...)
(unsafe (pub def-c-struct ...))
```

Mendeklarasikan struct dengan tata letak yang sama seperti di C. Ia hanya dapat ditulis di dalam
`unsafe` tingkat atas (yang tidak boleh berisi apa pun selain `def-c-struct`). Docstring dapat
ditaruh tepat setelah nama.

Tipe yang dapat ditulis untuk field adalah `i8` `i16` `i32` `u8` `u16` `u32` `c-long` `c-ulong` `f32`
`f64` `bool` `ptr`, pointer bertipe `(ptr T)`, dan `def-c-struct` lain (disematkan secara nilai).
Tata letak (offset tiap field, serta ukuran dan alignment struct) dihitung dengan aturan C (dengan
asumsi LP64). Field yang menunjuk struct itu sendiri dapat ditulis, tetapi struct tidak dapat
menyematkan dirinya sendiri.

```lisp
(unsafe
  (def-c-struct point (x i32) (y f64))              ; x at 0, y at 8, size 16
  (def-c-struct seg (a point) (b point) (next (ptr seg))))
```

Nama `def-c-struct` masuk ke ruang nama tipe (tidak ada `defstruct` atau sejenisnya yang bernama sama
dapat berada dalam modul yang sama), tetapi **ia bukan tipe sebuah nilai**. Anda tidak dapat menulis
`(defun f ((p point)) ...)`; ia muncul hanya sebagai apa yang ditunjuk oleh pointer bertipe.

**Pointer bertipe `(ptr T)`** adalah alamat yang menunjuk ke sebuah `T`. `T` adalah salah satu tipe
yang dapat ditulis untuk field di atas. Ia adalah word mesin mentah seperti `ptr`, dengan aturan yang
sama untuk tempat kemunculannya (hanya argumen, tipe kembalian, dan variabel lokal; ia dapat menjadi
nilai hanya di dalam `unsafe`).

Alokasi, pembacaan, dan penulisan ditulis dalam bentuk berikut. Semuanya hanya dapat dipakai di dalam
`unsafe`.

| Bentuk | Arti |
|---|---|
| `(c-alloc T)` / `(c-alloc T n)` | Mengalokasikan `n` nilai `T` (1 jika dihilangkan). Isinya diisi 0. Mengembalikan `(ptr T)` |
| `(c-ref p i)` | Pointer ke elemen `i` dari `p`. Kesalahan jika di luar rentang yang dialokasikan |
| `(c-deref p)` / `(setf (c-deref p) v)` | Membaca / menulis skalar yang ditunjuk `p` |
| `p::field` / `(setf p::field v)` | Membaca / menulis field sebuah struct. Membaca field yang berupa struct tersemat memberikan alamatnya (`(ptr inner-type)`) |
| `(as ptr p)` | Melupakan tipe, menjadikannya `ptr` (untuk diserahkan ke sesuatu seperti `void *` pada `qsort`). Tidak ada konversi balik |

```lisp
(defun sum-x ((n int)) int
  (unsafe
    (let ((ps (c-alloc point n)))
      (dotimes (i n)
        (let ((p (c-ref ps i)))
          (setf p::x (as i32 i))))
      (let ((total 0))
        (dotimes (i n)
          (let ((p (c-ref ps i)))
            (setf total (+ total (as int p::x)))))
        total))))
```

**Memori yang dialokasikan dibebaskan ketika kendali meninggalkan `unsafe` yang mengalokasikannya.**
Pemiliknya adalah `unsafe` terluar secara leksikal dalam fungsi yang sama. Ia dibebaskan baik kode
selesai secara normal maupun ditinggalkan oleh `panic`, `throw`, atau `return-from`. Fungsi `lambda`
dan `labels` adalah fungsi terpisah, sehingga `c-alloc` di dalamnya memerlukan `unsafe` tersendiri di
dalamnya.

Karena itu, pointer bertipe tidak dapat meninggalkan `unsafe` yang mengalokasikannya. Masing-masing
berikut adalah kesalahan tipe:

- Menjadikannya nilai ekspresi `unsafe` (sehingga tidak dapat dikembalikan dari fungsi juga)
- Menangkapnya dalam closure (`lambda`, `labels`)
- Menyerahkannya ke `task` / `thread`
- Melemparnya dengan `throw`

Untuk memakai nilai di luar `unsafe`, salin ke `defstruct` atau bilangan di dalam `unsafe` dan
kembalikan itu.

**Memori yang dialokasikan di sisi C tidak ditangani.** Nilai yang masuk dari C sebagai pointer
bertipe (nilai kembalian `defffi`, argumen callback, nilai yang dibaca dari field bertipe pointer)
diperiksa saat dijalankan apakah menunjuk ke nilai bertipe itu dalam alokasi `c-alloc` yang masih
hidup, dan merupakan kesalahan jika tidak. NULL juga kesalahan. Untuk menerima memori yang
dialokasikan C, atau NULL, gunakan `ptr` tak bertipe (yang isinya tidak dapat dibaca).

```lisp
(unsafe (def-c-struct item (key i32) (tag u8)))
(defffi (c-qsort "qsort") (ptr c-ulong c-ulong (fn ((ptr item) (ptr item)) i32)) ())

(unsafe
  (let ((xs (c-alloc item 4)))
    ...
    (c-qsort (as ptr xs) 4 8 (lambda ((a (ptr item)) (b (ptr item))) i32 (- a::key b::key)))
    ...))
```

Ketika argumen sebuah callback ditolak oleh pemeriksaan, hal itu dilaporkan kepada pemanggil ketika
fungsi C kembali, sama seperti kegagalan di dalam callback.

### 3.4 defvar / defparameter / defconstant — variabel global

```lisp
(defvar (name Type) init-expr)        ; initializes only if not yet bound
(defparameter (name Type) init-expr)  ; assigns every time
(defconstant (name Type) init-expr)

; with a docstring (in the same order as CL's defvar/defparameter/defconstant: after the value)
(defvar (name Type) init-expr "doc")
(defconstant (name Type) init-expr "doc")
```

**Perbedaan `defvar` dan `defparameter` terlihat saat dimuat ulang** (seperti di CL). Jika variabel
global **sudah terikat, `defvar` bahkan tidak mengevaluasi penginisialisasinya**, sehingga ketika
Anda menyunting berkas pengaturan dan membacanya lagi, nilai yang diubah sesi tetap seperti
adanya. `defparameter` menugaskan setiap kali, sehingga membacanya lagi mengembalikan nilai ke apa
yang tertulis.

Anotasi tipe wajib (tidak disimpulkan dari penginisialisasi). `defvar` dapat diubah; `defconstant`
tidak dapat (`setf` adalah kesalahan).

### 3.5 defmethod — definisi metode

```lisp
; instance method: can be called as (m obj args...)
(defmethod name ((self Type) (arg Type2) ...) RetType body...)

; static / associated function: can be called as (Type::name args...)
(defmethod name (Type (arg Type2) ...) RetType body...)
```

Pemanggil menyelesaikan metode dari tipe statis `obj` (dispatch tunggal, statis). Docstring dapat
ditaruh pada posisi yang sama dan dengan aturan yang sama seperti pada `defun` (tepat setelah klausa
`where`, di awal badan, hanya ketika bentuk badan menyusul). Hal yang sama berlaku untuk metode di
dalam `impl`; docstring diambil dengan `(documentation Type::method)`.

Parameter tipe milik metode sendiri ditulis di namanya dengan `<...>`, sama seperti `defun`.
Parameter tipe dari tipe penerima (`T` di bawah) ditentukan oleh penerima; parameter milik metode
(`U`) disimpulkan dari argumen setiap pemanggilan.

```lisp
(defstruct Box<T> (v T))

(defmethod fmap<U> ((self Box<T>) (f (fn (T) U))) Box<U>
  (Box::new (f self::v)))

(fmap (Box::new 3) (lambda ((x int)) string (format false "~a" x)))   ; Box<string>
```

- Beri parameter tipe milik metode nama yang berbeda dari parameter tipe yang dideklarasikan tipe
  penerima (`T` pada `(defstruct Box<T> ...)`) maupun dari nama yang ditulis di penerima.
- Jika tipe penerima generik, tulis di penerima semua parameter tipenya sebagai variabel (`Box<T>`)
  atau semuanya sebagai tipe konkret (`Box<int>`).
- Metode di dalam `impl` tidak dapat menambah parameter tipe: signaturnya mengikuti yang
  dideklarasikan trait.

### 3.6 defstruct — struct (tipe buatan pengguna)

```lisp
(defstruct Name
  (field1 Type1)
  (pub field2 Type2)
  ...)

; generic (type parameters in angle brackets)
(defstruct Name<T1,T2...>
  (field TypeUsingT1)
  ...)
```

- Setiap field adalah `(name type)` atau `(pub name type)` (visibilitas per field, bebas dari `pub`
  milik struct itu sendiri). Satu ekspresi lagi di akhir menjadi **nilai bawaan** slot itu
  (`(x i32 0)`); lihat daftar opsi di bawah.
- Hal berikut dihasilkan secara otomatis:
  - Konstruktor `Name::new` (argumen berurutan field)
  - Getter `(field-name instance)`, dengan gula sintaks `instance::field-name`
  - Setter `(set-field-name instance value)`, dengan gula sintaks
    `(setf instance::field-name value)`
- Untuk menjadikan struct itu sendiri `pub`, taruh `pub` di depan, seperti `(pub defstruct ...)`.
- **Definisikan tipe sebelum menamainya.** Tipe field dapat berupa struct itu sendiri
  (`(next Option<node>)`), tetapi bukan tipe yang didefinisikan kemudian: tipe tidak memiliki
  deklarasi maju yang sepadan dengan `defsignature`. Nama yang belum didefinisikan menghasilkan
  kesalahan `unknown type` yang sama pada tipe argumen `defun` atau pada `the`. Jadi dua tipe yang
  saling merujuk tidak dapat ditulis.
- **Variabel tipe hanyalah yang ditulis pada posisi pendeklarasian.** Untuk
  `defun`/`defstruct`/`defenum`/`deftype`, `<T>` pada nama; untuk `defmethod`, tipe penerima
  (`(self box<T>)`, atau `box<T>` untuk metode statis) dan `<U>` pada nama metode; untuk `impl`,
  tipe sasaran dan `impl<T>`; untuk `deftrait`, `Self` dan tipe terkait pada `(type Item)`. Nama
  yang pertama kali muncul di tempat lain mana pun (argumen, nilai kembalian, `the`/`lambda` di
  badan) tidak menjadi variabel tipe; ia adalah `unknown type`.
- **Docstring**: literal string tepat setelah nama, sebelum field, menjadi docstring
  (`(defstruct Name "doc" (field Type)...)`, posisi yang sama seperti `defstruct` pada CL). Sebuah
  field selalu berbentuk `(name Type ...)` dan tidak pernah dapat berupa string polos, sehingga tidak
  ada ambiguitas. Ambil dengan `(documentation Name)`.

#### Daftar opsi

Menulis daftar `(Name option...)` pada posisi nama menentukan opsi (posisi yang sama seperti CL).

```lisp
(defstruct (point (:constructor make-point)          ; keyword constructor
                  (:constructor at (x &optional y))  ; BOA constructor
                  (:copier copy-point))
  (x i32 0)          ; a third element is that slot's default value
  (y i32 0))

(point::make-point :y 7)   ; x is 0
(point::at 1)              ; y is 0
(point::at 1 2)
(copy-point p)             ; a shallow copy (the same as CL's copier)
```

- **`:constructor`**: yang dihasilkan adalah **fungsi statis** pada tipe itu (`point::make-point`),
  yang badannya selalu `(point::new ...)`. `new` tetap satu-satunya konstruktor struktural; yang
  dibuat di sini adalah *cara memanggilnya*. Beberapa dapat dideklarasikan.
  - `(:constructor name)` menerima setiap slot sebagai `&key`. **Setiap slot memerlukan nilai
    bawaan** (bahasa ini tidak memiliki padanan "slot tak terikat" pada CL).
  - `(:constructor name (slot...))` menerima slot yang disebutkan sebagai argumen posisional
    (dalam urutan apa pun). Slot yang tidak disebutkan diisi nilai bawaannya, sehingga **memerlukan
    nilai bawaan**. Setelah `&optional`, sisanya boleh dihilangkan (dan juga memerlukan nilai
    bawaan).
- **`:copier`**: menghasilkan **metode instans** yang mengembalikan nilai baru dengan nilai slot yang
  sama. Dangkal, seperti copier pada CL.
- **`:include Parent`**: menaruh slot induk di depan (nilai bawaan juga diwarisi; induk boleh berada
  di berkas lain). **Ia tidak membuat hubungan tipe**: anak bukan subtipe dari induk, metode induk
  tidak berlaku pada anak, dan tidak ada pengujian saat dijalankan yang menghubungkan keduanya.
  Bahasa ini tidak memiliki subtyping; antarmuka bersama adalah tugas `deftrait`. Hanya *daftar* slot
  yang digabungkan.
- **Nilai bawaan slot hanya dibaca oleh konstruktor yang dihasilkan.** Menulis nilai bawaan tanpa
  mendeklarasikan `:constructor` apa pun adalah kesalahan, karena tidak akan pernah dapat dipakai.
- Opsi yang ditinggalkan, dan alasannya:
  - **`:conc-name`**: di CL ia memberi awalan pada pengakses untuk menghindari bentrok dalam satu
    ruang nama fungsi yang datar. Di sini, pengakses adalah metode yang di-dispatch berdasarkan tipe
    penerima, sehingga bentrok tidak terjadi, dan awalan akan merusak `instance::field` (yang hanya
    mengenal nama slot).
  - **`:predicate`**: menjawab saat dijalankan "apakah nilai ini sebuah `point`?". Di sini tipe
    adalah klasifikasi saat kompilasi tanpa saksi saat dijalankan, dan tidak ada posisi tempat "nilai
    bertipe tak dikenal yang mungkin sebuah point" ada (`match` pada `Sexpr` tertutup, dan `:dyn`
    tidak dapat di-downcast), sehingga predikat yang dihasilkan hanya akan selalu mengembalikan
    `true`.
  - **`:type` / `:initial-offset` / `:named`**: semuanya mengganti representasi nilai dengan daftar
    atau vector. Representasi adalah milik kompiler dan tidak dapat diamati dari bahasa.

### 3.7 defenum — enum (sum type)

```lisp
(defenum Name
  (Variant1 Type1 Type2...)   ; a variant with a payload (positional fields)
  (Variant2)                  ; a variant without a payload
  ...)

; generic
(defenum Option<T>
  (Some T)
  (None))
```

- Setiap varian berbentuk `(VariantName FieldType...)`. Field hanya posisional (tidak memiliki nama).
  Setidaknya satu varian diperlukan, dan nama tidak boleh berulang.
- Nilai dibangun, seperti pada `Option`/`Result` bawaan, secara terkualifikasi atau melalui `use`:
  `(Name::Variant1 a b)`, atau `(Variant1 a b)` setelah `(use Name)`.
- Nilai dapat dibongkar dengan `match` / `if-let`. `match` memeriksa kelengkapan (harus mencakup
  setiap varian atau memiliki `_`):
  ```lisp
  (match opt
    ((Some v) v)
    ((None) 0))
  ```
- Metode dan fungsi terkait ditambahkan kemudian dengan `defmethod`/`impl`, seperti pada `defstruct`.
- Untuk menjadikan enum itu sendiri `pub`, tulis `(pub defenum ...)`.
- **Docstring**: posisi dan aturan yang sama seperti `defstruct`, tepat setelah nama, sebelum varian
  (`(defenum Name "doc" (Variant ...)...)`). Ambil dengan `(documentation Name)`.

### 3.8 deftype — alias tipe

```lisp
(deftype meters i32)
(deftype fallible<T> Result<T,string>)
(deftype pred (fn (i32) bool))

(defun double ((m meters)) meters (* m 2))
(defun parse ((s string)) fallible<i32> ...)
```

`deftype` pada CL, dipersempit menjadi yang masuk akal pada bahasa bertipe statis: **sebuah ejaan
untuk tipe, bukan sebuah tipe**.

- Posisi nama sama seperti `defun`, dan argumen generik ditulis `Name<T,U>`. Pada tempat pemakaian,
  diperlukan tepat sejumlah argumen tipe yang dideklarasikan (terlalu banyak atau terlalu sedikit
  adalah kesalahan saat itu juga).
- Ekspansi terjadi **di dalam pengurai tipe**. Jadi tidak ada hal di hilir yang mengetahui alias itu
  ada: kunci monomorfisasi, dump, jalur kompilasi, dan **pesan kesalahan** semuanya menampilkan
  bentuk yang sudah diekspansi. Jika `(f "x")` gagal terhadap fungsi yang memerlukan `meters`, pesan
  menyebut `i32`.
- **Ia bukan tipe baru.** `(deftype meters i32)` membuat `meters` dan `i32` menjadi tipe yang sama,
  sehingga tertukar tidak terdeteksi. Jika Anda ingin keduanya dipisahkan, gunakan `defstruct`.
- **Ia bukan predikat.** `(deftype small () '(integer 0 9))` pada CL menggambarkan *himpunan nilai*
  yang diuji `typep` saat dijalankan, tetapi di sini tipe adalah klasifikasi saat kompilasi tanpa
  saksi saat dijalankan, sehingga alias yang membatasi nilai tidak akan memiliki apa pun untuk
  dibatasi.
- **Ia tidak dapat mengandung dirinya sendiri.** Alias diekspansi di tempat ia ditulis, sehingga
  tidak ada tempat baginya untuk berekursi. Tipe data rekursif ditulis dengan `defstruct`/`defenum`.
- Ia berbagi ruang nama dengan tipe dan trait (dalam satu modul ia tidak dapat bernama sama dengan
  `defstruct`/`defenum`/`deftrait`). Jadikan publik dengan `(pub deftype ...)` dan bawa masuk dengan
  `(use m::meters)`.
- **Docstring**: tepat setelah nama, sebelum tipe (`(deftype Name "doc" Type)`).

### 3.9 deftrait / impl — trait

```lisp
(deftrait TraitName (SuperTrait...)      ; the supertrait list is required; () if none
  (type AssocName)                       ; associated types (any number, optional)
  (method-name ((self Self) params...) RetType)          ; no body = must be implemented
  (method-name ((self Self) params...) RetType body...)) ; with a body = default implementation

(impl TraitName TargetType
  (where (Trait A)...)                   ; bounds applying to the whole impl (optional)
  (type AssocName ConcreteType)          ; makes an associated type concrete
  (method-name (recv params...) RetType body...))
```

Melalui `impl`, setiap metode didaftarkan sebagai `defmethod` biasa milik `TargetType`. Trait
dirujuk sebagai batas trait pada klausa `where` fungsi generik (lihat
[3.1 defun](#31-defun--definisi-fungsi)). Nama trait juga dapat berupa path `::` seperti
`m::Trait`.

**Daftar supertrait (wajib)**: selalu ditulis tepat setelah nama trait. Setiap elemen adalah nama
trait polos, atau, jika trait itu memiliki tipe terkait, `(Trait (Assoc Type))` dengan **semua tipe
terkaitnya ditetapkan**.

```lisp
(deftrait Eq () ...)                       ; no supertraits
(deftrait Ord (Eq) ...)                    ; Rust's trait Ord: Eq
(deftrait CharSource ((Iter (Item char)))  ; pinning an associated type
  (rewind ((self Self)) ()))
```

Pewarisan memiliki tiga efek. (1) `impl Ord X` mensyaratkan `impl Eq X` ditulis **lebih dulu**
(aturan tentang urutan penulisan: satu-satunya bentuk yang dapat ditentukan secara deterministik di
REPL dan dengan `load` langkah demi langkah, dan lebih ketat daripada Rust). (2) `(where (Ord T))`
saja memungkinkan Anda memanggil metode `Eq` juga. (3) Metode `Eq` dapat dipanggil melalui `:dyn Ord`,
dan nilai `:dyn Ord` dapat diserahkan apa adanya di tempat `:dyn Eq` diperlukan (upcasting). Subtrait
yang mendeklarasikan ulang metode bernama sama dengan induknya, dan pewarisan metode bernama sama
dari dua induk, keduanya adalah kesalahan (vtable memiliki satu slot per nama). Pewarisan berlian
digabung menjadi satu slot.

**Implementasi bawaan**: badan setelah signature dipakai ketika `impl` menghilangkan metodenya. Badan
diselesaikan di **ruang nama modul** tempat trait ditulis, sehingga ia dapat memanggil fungsi tidak
publik dari modul itu. Metode dengan badan juga dapat memiliki klausa `where` dan docstring. Badan
diperiksa tipenya **sekali, pada titik deklarasi**, dengan `Self` dibiarkan sebagai variabel tipe
(dibatasi oleh `Self: trait itu sendiri`), seperti di Rust: kesalahan yang akan gagal untuk setiap
`impl` dan setiap tipe pengimplementasi, bahkan pada bawaan yang tidak pernah dihilangkan `impl`
mana pun, tertangkap di sana. Pemanggilan pada `self` ke metode trait itu sendiri atau supertrait-nya
melewati batas ini, dan tipe terkait ditetapkan ke dirinya sendiri, sehingga signature yang
mengembalikan `Item` dicocokkan dengan badan tanpa mengetahui tipe konkretnya.

**Implementasi blanket**: menjadikan sasaran sebagai variabel tipe mengimplementasikan trait
sekaligus untuk setiap tipe yang memenuhi batas.

```lisp
(deftrait Clamp (Ord)
  (clamp ((self Self) (lo Self) (hi Self)) Self
    (if (less self lo) lo (if (less hi self) hi self))))
(impl<T> Clamp T (where (Ord T)))          ; no body at all; everything is the default
```

**Tidak ada kode yang dihasilkan sampai tipe konkret benar-benar memakainya** (sekali per tipe,
dengan mekanisme yang sama seperti monomorfisasi biasa). Sebuah trait paling banyak memiliki satu
implementasi blanket. Jika sebuah tipe memiliki `impl` eksplisit, yang itu diutamakan. Pemeriksaan
tipe badan terpisah dari pembangkitan: ia dilakukan sekali pada titik deklarasi, **dengan sasaran
dibiarkan sebagai variabel tipe** (seperti di Rust), sehingga bahkan implementasi yang tidak pernah
dipakai pun kesalahannya tertangkap di sana jika akan gagal untuk setiap sasaran di bawah batas yang
dideklarasikan. Pemanggilan yang dibenarkan oleh batas (`(less self other)` di bawah
`(where (Ord T))` dan sebagainya) lolos, seperti pada badan `defun` generik.

**Docstring**: `deftrait` dapat memiliki satu docstring untuk seluruh trait, sebagai literal string
tepat setelah daftar supertrait, sebelum butir-butir
(`(deftrait Name () "doc" (type ...) (method ...)...)`). Signature tanpa badan tidak dapat memiliki
docstring: string di akhir akan menjadi nilai kembalian implementasi bawaan itu sendiri, sehingga
keduanya tidak dapat dibedakan.

Trait yang disediakan pustaka standar: **`Iter`** (`next` / tipe terkait `Item`; dasar `doiter` dan
fungsi sekuens), **`Eq`** (`equals`; `not-equals` adalah implementasi bawaan), **`Ord`** (mewarisi
`Eq`; hanya `less` yang wajib diimplementasikan, dan `less-equal` / `greater` / `greater-equal`
adalah implementasi bawaan), **`Error`** (`message` / `source`; `:dyn Error` untuk menangani tipe
kesalahan secara seragam), **`print-object`** (representasi cetak per tipe), **`Pathish`**
(pathname designator: string atau `pathname`), dan hierarki stream **`Stream`** → **`InputStream`** /
**`OutputStream`** → **`CharInput`** / **`CharOutput`** → **`PeekInput`**. Tipe mana yang
mengimplementasikan trait mana ada di [types.md](types.md); metode tiap trait ada di
[Trait Standar](functions/traits.md),
[Tipe kesalahan](functions/option-result.md#3-tipe-kesalahan-dan-trait-error),
[print-object](functions/printing.md#5-print-object-representasi-cetak-per-tipe), dan
[Stream](functions/streams-files.md). Jika Anda meng-`impl` `Iter` untuk tipe koleksi Anda sendiri,
`doiter` (bab 5) dan `map` / `filter` / `sort` dan sejenisnya bekerja padanya apa adanya.

Pemanggilan trait bersifat **statis** secara bawaan (diselesaikan berdasarkan tipe statis penerima).
Untuk menangani nilai yang tipe konkretnya ditentukan saat dijalankan, tipe objek trait `:dyn Trait`
(bab 2) memberikan dispatch dinamis melalui vtable:

```lisp
(deftrait Drawable () (draw ((self Self)) string))
(defstruct circle (r i32))
(defstruct square (side i32))
(impl Drawable circle (draw ((self Self)) string "circle"))
(impl Drawable square (draw ((self Self)) string "square"))

(defun render-all ((xs Vector<:dyn Drawable>)) ()
  (doiter (d (iter xs)) (println "~a" (draw d))))   ; one call site, an answer per implementation
```

Hanya trait yang "setiap metodenya memiliki penerima `self`, tidak memakai `Self` di tempat lain
selain penerima, dan itu sendiri bukan generik maupun variadik" yang dapat dijadikan `:dyn` (metode
yang diwarisi harus memenuhi syarat yang sama).

Hanya tipe yang nilainya memiliki representasi di heap yang dapat masuk ke kotak `:dyn`:

| Dapat masuk | Tidak dapat masuk |
|---|---|
| Tipe `defstruct` / `defenum` (termasuk `Vector<T>`, `cons-cell<A,B>`, `Result<T,E>` dan struct pustaka standar), `HashTable<K,V>`, `Sexpr`, `int`, `ratio`, `f64`, `string`, `random-state` | Bilangan bulat berlebar tetap (`i8` sampai `u32`), `f32`, `bool`, `char`, `symbol`, `()`, tipe fungsi, dan `Option<T>` tanpa kotak ([representasi Option saat dijalankan](functions/option-result.md#2-representasi-optiont-saat-dijalankan)) |

Menaruh nilai bertipe yang tidak dapat masuk di tempat `:dyn` diharapkan adalah kesalahan tipe.
Untuk menangani nilai seperti itu melalui `:dyn`, bungkuslah dalam struct, seperti
`(defstruct flag (v bool))`.

### 3.10 module / use — ruang nama

```lisp
(module path body...)      ; path is a sequence of segments such as foo or foo::bar
(in-module path)           ; from here to the end of this unit, inside path (the flat form of module)
(use path...)              ; alias functions, types and modules into the current namespace
(import path...)           ; the same as use (a CL-compatible spelling)
(shadowing-import path...) ; a use that knowingly takes a bare name already in use
```

- `module` membuat ruang nama. **Tipe bukan ruang nama** (seperti di Rust, tipe hanya memiliki fungsi
  terkait dan metode).
- Meng-`use` sebuah tipe membuat konstruktor dan metode statis publiknya tersedia dengan nama polos
  juga (misalnya, setelah `(use option)`, `some`/`none` dapat dipanggil tanpa
  `option::some`/`option::none`).
- Urutan penyelesaian nama polos (pengenal tak terkualifikasi): bentuk khusus → konstruktor →
  fungsi bebas (ruang nama saat ini → akar) → metode instans (diselesaikan berdasarkan tipe statis
  argumen pertama). Ia tidak naik melalui modul induk di antaranya.
- Path terkualifikasi `a::b` menyelesaikan `a` dengan urutan di atas; jika ia modul, penyelesaian
  masuk ke dalamnya, dan jika ia tipe, segmen terakhir diselesaikan sebagai butir terkait.
- **`use` memengaruhi bentuk-bentuk sesudahnya.** Berkas dibaca satu bentuk sekali waktu, dan
  dependensi diselesaikan tepat sebelum bentuk diperiksa, sehingga menulis `m::f` **di atas**
  `(use m)` menghasilkan `unresolved path`. Taruh `use` di awal berkas.
- **`use` dapat menerima beberapa path** (`(use a::f b::g)`). `import` adalah ejaan yang kompatibel
  dengan CL dengan perilaku yang sama.
- **`use` yang nama polosnya sudah terpakai dilaporkan.** Menyelesaikan nama polos melihat definisi
  milik modul itu sendiri sebelum alias, sehingga `(use m::twice)` setelah `(defun twice ...)`
  **tidak melakukan apa-apa**. Jika Anda memang bermaksud, tulis `shadowing-import` (ia tetap tidak
  dapat mengalahkan definisi, karena tidak ada cara menghapusnya; ia hanya mengalahkan alias
  sebelumnya).
- **`in-module` adalah bentuk datar dari `(module path body...)`.** Menulis `(in-module geometry)`
  menaruh semua dari situ sampai akhir unit (berkas, atau badan `module` yang melingkupi) di dalam
  `geometry`. Ia berada **di dalam** modul milik berkas itu sendiri (`main::geometry` untuk
  `main.typl`). Dua berturut-turut bersarang secara berurutan. Ia berbeda dari `in-package` pada CL,
  dan dinamai berbeda: di sistem ini berkas sudah merupakan modul, sehingga tidak ada yang perlu
  "dipilih", dan yang dapat dilakukan sebuah bentuk hanyalah bersarang.

### 3.11 Berkas dan modul (proyek multiberkas)

Path berkas relatif terhadap akar sumber adalah path modul: isi `<root>/geo/point.typl` secara
implisit dibungkus dalam modul `geo::point` (direktori juga satu segmen, gaya Rust / Python).
`(module bar ...)` eksplisit di dalam berkas bersarang **di dalamnya** (`geo::point::bar`), sehingga
path turunan dan deklarasi eksplisit tidak pernah bertabrakan.

- **Akar sumber**: taruh berkas manifes `typelisp.toml` di akar proyek (boleh kosong; secara opsional
  satu baris `src = "src"` menamai direktori sumber). Ia ditemukan dengan naik dari direktori berkas
  sasaran. Tanpa manifes, direktori berkas masuk (direktori saat ini untuk REPL) adalah akarnya.
- **Pemuatan sesuai permintaan**: ketika `(use geo::point)` merujuk modul yang belum dimuat, berkas
  yang sesuai (`geo/point.typl`) dimuat, diperiksa tipenya, dan didaftarkan secara otomatis.
  `use a::b::c` mencari awalan terpanjang lebih dulu: `a/b/c.typl` → `a/b.typl` → `a.typl` (karena
  `c` mungkin butir di dalam modul). Definisi yang terlihat dari modul lain memerlukan `pub`
  ([3.13 pub](#313-pub--visibilitas)).
- **Rujukan melingkar adalah kesalahan**: rantainya dilaporkan dalam bentuk
  `circular module dependency: a -> b -> a`.
- **Menjalankan**: `typl <file.typl>` menjalankan sebuah berkas (tanpa argumen, REPL). `use` di REPL
  menyelesaikan berkas dengan aturan yang sama.
- **Kapasitas arena cons**: `typl --heap-cells N` menyetel **kapasitas awal** arena sel cons (bawaan
  65536; bentuk `--heap-cells=N` juga berfungsi, baik untuk menjalankan berkas maupun REPL). Arena
  **membesar dengan menambah** ketika kekurangan. Batas pertumbuhannya adalah 256 kali kapasitas awal,
  dan alokasi di luarnya menghasilkan `heap exhausted`: kapasitas awal berarti "alokasikan sebanyak
  ini pada awalnya", dan batasnya berarti "di luar ini, anggap sebagai kebocoran".

### 3.12 load — pemuatan datar

```lisp
(load "path")   ; top level only; path is a string literal
```

- **Pemuatan datar** gaya CL: membaca bentuk-bentuk berkas sasaran **ke dalam ruang nama saat ini**
  apa adanya (tanpa membungkusnya dalam modul, tidak seperti `use`). Hanya tingkat atas (di dalam
  badan fungsi adalah kesalahan tipe).
- `path` relatif terhadap direktori berkas yang memuat (dari REPL, terhadap cwd proses). Jika tidak
  memiliki ekstensi, `.typl` ditambahkan.
- `(load ...)`/`(use ...)` di dalam berkas yang dimuat juga diproses secara rekursif.
- **Ia membaca satu bentuk sekali waktu dan menjalankannya saat itu juga** (seperti `load` pada CL).
  Bentuk *k* sudah selesai dijalankan sebelum *k+1* dibaca: bahkan jika ada kesalahan sintaks atau
  tipe di tengah, bentuk sebelumnya sudah dijalankan. Berkas modul yang dimuat oleh `use` berbeda:
  diperiksa sebagai satu kesatuan dan menjalankannya diserahkan kepada yang meng-`use`-nya (sepadan
  dengan `compile-file` pada CL).

### 3.13 pub — visibilitas

```lisp
(pub defun ...)
(pub defsignature ...)
(pub defffi ...)
(pub defvar ...)
(pub defparameter ...)
(pub defconstant ...)
(pub defmacro ...)
(pub defmethod ...)
(pub defstruct ...)
(pub defenum ...)
(pub deftype ...)
```

`pub` hanya dapat ditaruh pada sebelas jenis di atas (tidak pada `module`/`use`/`deftrait`/`impl`).
Ia ditulis dengan kata kunci definisi tepat setelah `pub`, bukan dalam bentuk `(pub (defun ...))`
yang membungkus definisi dengan tanda kurung. Satu `pub` memublikasikan tepat satu definisi
(beberapa definisi tidak dapat ditandai sekaligus).

### 3.14 defmacro — definisi makro

```lisp
(defmacro name (required... &optional opt... &rest rest-name &key key...) body...)
```

- Semua parameter dan nilai kembalian selalu `Sexpr`, sehingga tidak ada anotasi tipe yang ditulis.
- Makro tidak higienis gaya CL (menghindari bentrok dengan `gensym` adalah tanggung jawab penulis
  makro).
- Daftar lambda mengikuti urutan CL `required &optional &rest &key` (setiap penanda paling banyak
  sekali, dan hanya dalam urutan ini).
  - `&optional` … argumen opsional. `name` atau `(name default-expr)`. Ekspresi bawaan dievaluasi
    saat ekspansi (ia dapat merujuk parameter yang terikat lebih awal) dan diikat ketika argumen
    dihilangkan (tanpa bawaan, daftar kosong `()`).
  - `&rest name` … menerima argumen posisional yang tersisa bersama-sama sebagai satu daftar `Sexpr`.
  - `&key` … argumen keyword. `name` atau `(name default-expr)`. Pemanggil mengoperkannya sebagai
    `:name value` (dalam urutan apa pun). Ketika dihilangkan, ekspresi bawaan (daftar kosong `()`
    jika tidak ada). Keyword yang tidak dikenal atau urutan `:key` berpanjang ganjil adalah
    kesalahan.
- Contoh: `(defmacro pair (x &optional (y 1)) ...)` / `(defmacro make (&key (a 0) (b 9)) ...)`.

### 3.15 macrolet / symbol-macrolet — pengikatan makro lokal

```lisp
(macrolet ((name (lambda-list) body...) ...) body...)   ; lexically scoped macros
(symbol-macrolet ((name expansion) ...) body...)         ; a name stands for a form
```

Keduanya adalah bentuk khusus **ekspresi**, dan tidak ada yang tersisa saat dijalankan (yang
dikompilasi adalah bentuk ekspansi dari badan). Daftar lambda-nya sama seperti `defmacro`. Aturan
terperinci dan contohnya ada di
[Pengikatan makro lokal](functions/system.md#9-pengikatan-makro-lokal-macrolet--symbol-macrolet).

## 4. Pengikatan dan percabangan

```lisp
(let ((name val) ...) body...)      ; parallel binding
(let* ((name val) ...) body...)     ; sequential binding (earlier bindings usable in later initializers)

(if cond then else)                 ; else is required (always three elements)
(when cond body...)                 ; an if without else (Unit type). defmacro
(unless cond body...)               ; the negation of when. defmacro
(cond (test1 body...) (test2 body...) ... (else body...))   ; defmacro
(case expr
  (key1 body...)
  ((key2 key3) body...)             ; a list of keys: matches if any of them does
  (else body...))                   ; expr is evaluated once. keys are compared with equal.
                                     ; keys are "literals" and are not evaluated (as in CL).
                                     ; a bare symbol a means the symbol 'a.
                                     ; writing 'a is an error (use the bare a). defmacro
(ecase expr (key body...) ...)      ; a case requiring a match. panics if nothing matches. defmacro
(ccase expr (key body...) ...)      ; CL's ccase. there are no restarts to offer, so it is the same as ecase. defmacro
(and expr...)                       ; short-circuit evaluation. true with zero arguments. defmacro
(or expr...)                        ; short-circuit evaluation. false with zero arguments. defmacro
(progn body...)                     ; runs in order and returns the last value
(unsafe body...)                    ; the same as progn, plus permission to write FFI calls
                                     ; and raw words. see 3.3 defffi
(prog1 form more...)                ; evaluates everything; the value is that of form. defmacro
(prog2 a b more...)                 ; evaluates everything; the value is that of b. defmacro
(the Type expr)                     ; a type annotation (no run-time effect)
```

### 4.1 unsafe — menanggung asumsi yang tidak dapat diperiksa

```lisp
(unsafe body...)
```

Sama seperti `progn`: mengevaluasi badan secara berurutan dan mengembalikan nilai terakhir. Ia tidak
membuat lingkup dan bukan batas fungsi (`break` / `return-from` langsung lewat ke luar). Bedanya
adalah beberapa hal hanya dapat ditulis di dalamnya.

Tiga hal saat ini memerlukan `unsafe`: memanggil fungsi C yang dideklarasikan dengan
[defffi](#33-defffi--deklarasi-fungsi-c-ffi), menjadikan word mesin mentah (`ptr` / `c-long` /
`c-ulong` / `(ptr T)`) sebagai nilai, dan
[`def-c-struct` dan `c-alloc`](#def-c-struct-dan-pointer-bertipe--mengalokasikan-struct-c).

Memori yang dialokasikan dengan `c-alloc` dibebaskan saat meninggalkan `unsafe` terluar dalam fungsi
yang sama. Hanya `unsafe` itu, tidak seperti `progn`, yang memiliki pekerjaan saat keluar:
membebaskan.

Yang ditanggung `unsafe` adalah asumsi berikut yang tidak dapat diverifikasi kompiler:

- **Bahwa tipenya cocok.** Bahwa signature C yang dideklarasikan cocok dengan yang sebenarnya. Jika
  tidak, argumen masuk ke register yang salah dan nilai kembalian dibaca pada lebar yang salah.
- **Keamanan memori.** Apa yang dilakukan sisi C dengan apa yang diberikan kepadanya.
- **Keadaan seluruh proses.** Variabel lingkungan, penangan sinyal, `errno`. Misalnya, memanggil
  `setenv` melalui FFI merusak asumsi yang dibuat `decode-universal-time` implementasi ini ketika
  menghitung waktu lokal.
- **Keamanan thread.**

Ia bukan jalan keluar dari pemeriksaan tipe. `(unsafe (+ 1 "two"))` tidak lolos. Yang diizinkan
adalah menulis **operasi** tertentu, bukan menulis omong kosong.

Ia bekerja secara leksikal. Badan `lambda` yang ditulis di dalam `unsafe` mewarisi izinnya (seperti
closure di dalam blok `unsafe` pada Rust). Nilainya mungkin kemudian dipanggil dari luar `unsafe`,
tetapi menuliskannya di sana sendiri dianggap menerima tanggung jawab.

### 4.2 destructuring-bind — membongkar daftar berdasarkan bentuk

```lisp
(destructuring-bind lambda-list form body...)
```

Membongkar daftar yang dihasilkan `form` **berdasarkan bentuknya** dan mengikatnya. Daftar lambda-nya
adalah milik `defmacro` (required → `&optional` → `&rest`/`&body` → `&key`, masing-masing dengan
ekspresi bawaan), untuk alasan yang sama CL membagi satu untuk keduanya: keduanya adalah dua bentuk
yang membongkar hal yang sama.

```lisp
(destructuring-bind (op a b) (quote (+ 1 2)) (format false "~a ~a ~a" op a b))  ; "+ 1 2"
(destructuring-bind (head &rest tail) xs (format false "~a | ~a" head tail))
(destructuring-bind (a &optional (b 9)) (quote (1)) b)                          ; 9
(destructuring-bind (&key (x 0) y) (quote (:y 7)) (format false "~a ~a" x y))   ; "0 7"
```

- **Setiap variabel yang terikat adalah `Option<Sexpr>`.** Ini bukan keterbatasan implementasi
  melainkan sifat apa yang diikat: daftar S-expression adalah satu-satunya daftar di bahasa ini,
  sehingga tidak ada tipe lain untuk diberikan kepada elemen. Kembali ke `match` di tempat skalar
  diperlukan sama seperti pada badan `defmacro`.
- **Bentuk yang tidak cocok melakukan panic** (sepadan dengan kesalahan pada CL): elemen terlalu
  sedikit atau terlalu banyak, urutan `&key` berpanjang ganjil, atau keyword yang tidak dikenal.
  `sexpr-car` adalah fungsi longgar yang mengembalikan `()` untuk `()`, sehingga tanpa pemeriksaan,
  daftar yang pendek akan diam-diam terikat ke urutan kosong.
- **Daftar lambda bersarang tidak didukung.** `defmacro` juga tidak menerimanya, sehingga hanya ada
  satu aturan. `(a (b c))` tidak diam-diam mengikat sublist ke `b`; ia adalah kesalahan yang
  menyatakannya.
- Ekspresi bawaan `&optional` / `&key` **dievaluasi hanya ketika dipakai** (seperti di CL).
- Tidak ada yang sepadan dengan `&allow-other-keys` pada CL (`defmacro` juga tidak memilikinya).

### 4.3 match — pencocokan pola

```lisp
(match expr
  (pattern body...)
  ...)
```

Jenis pola:
- `_` — wildcard
- Nama variabel — pola pengikatan (selalu cocok). Namun, jika tipe scrutinee memiliki varian bernama
  itu, ia diselesaikan sebagai **pola nama varian polos di bawah**
- Nama varian polos — cocok dengan varian yang tidak menerima argumen
  (`(match c (red 1) (blue 2))`). Menulis varian yang memiliki field dengan nama polosnya adalah
  kesalahan arity, jadi tulis dalam kurung, seperti `(circle r)`
- **Literal langsung**: bilangan bulat / `true`/`false` / karakter — dibandingkan sebagai word
- **Literal nilai**: string / bilangan floating-point / simbol (`'foo`) / bilangan bulat bignum /
  rasio — dibandingkan berdasarkan nilai dengan `Eq::equals` tipe itu
  ([Trait Standar](functions/traits.md#2-eq--ord-perbandingan)). String dibandingkan berdasarkan isi,
  bukan identitas
- `(= expr)` — mengevaluasi ekspresi apa pun dan membandingkan dengan `Eq::equals`. Satu-satunya cara
  membandingkan tipe yang tidak memiliki sintaks literal (instans `defstruct`, variabel global, hasil
  komputasi), dan implementasi `Eq` buatan pengguna menjadi aturan perbandingan apa adanya. `expr`
  dapat merujuk apa pun yang terlihat dari posisi cabang (argumen, pengikatan luar, variabel global)
- `(Ctor sub-pattern...)` — pola konstruktor (`Some x` `None` `Cons a d` `Ok v` dan sebagainya)
- `(:or p1 p2 ...)` — pola atau: cocok bila salah satu alternatif cocok. Badannya hanya satu, jadi
  setiap alternatif harus mengikat variabel yang sama dengan tipe yang sama. Dapat juga ditulis di
  dalam pola konstruktor (`(some (:or (circle r) (rect r _)))`)

Membandingkan tipe yang tidak mengimplementasikan `Eq` dengan literal nilai / `(= expr)` adalah
kesalahan tipe (bahasa ini memilih untuk mengatakan "ini tidak dapat dibandingkan" daripada
membiarkan cabang yang diam-diam tidak pernah cocok).

**Penjaga**: menulis `:when kondisi` setelah pola memilih cabang hanya bila pola cocok dan
kondisinya juga benar. Bila kondisinya salah, cabang berikutnya dicoba. Kondisi dapat membaca
variabel yang diikat pola.

```lisp
(defun classify ((n int)) string
  (match n
    (0 "zero")
    (k :when (< k 0) "negative")
    (k :when (evenp k) "even")
    (_ "odd")))
```

- Cabang berpenjaga tidak dihitung untuk kelengkapan (seperti di Rust). Kondisinya bisa salah, jadi
  varian yang akan dicakupnya juga memerlukan cabang tanpa penjaga atau `_`.
- Pola atau dan penjaga dapat digabung. Penjaga dievaluasi alternatif mana pun yang cocok
  (`((:or 1 2 3) :when on "small")`).

**Literal nilai terhadap scrutinee `Sexpr`**: `Eq` pada `sexpr` adalah `eq` (identitas pada CL),
sehingga literal langsung (`'foo` (di-intern) / bilangan bulat / karakter / `true`/`false`) dapat
ditulis apa adanya dan cocok berdasarkan isi:

```lisp
(match s ('add 1) (42 2) (#\a 3) (_ 0))
```

Literal non-langsung (string / bilangan floating-point / bilangan bulat bignum / rasio) **tidak dapat
ditulis** terhadap `Sexpr`. `eq` pada literal itu membandingkan identitas objek, yang akan membuat
"cabang yang lolos pemeriksaan tipe tetapi tidak pernah cocok", sehingga ia adalah kesalahan yang
menyebut pola varian: tulis `(str "hi")` dan ia dibongkar menjadi `string` lalu dibandingkan
berdasarkan isi. `(= expr)` secara eksplisit meminta `equals`, sehingga batasan ini tidak berlaku
padanya.

**Scrutinee tidak harus berupa ADT.** `string`/`symbol`/`i32`/`f64` dan sejenisnya dapat dicocokkan
langsung (di situlah pola literal string berada). Namun, tipe tanpa varian tidak dapat dicakup dengan
enumerasi, sehingga `_` (atau pola pengikatan yang bertindak sebagai wildcard) diperlukan:

```lisp
(defun kind ((s string)) i32
  (match s
    ("add" 1)
    ("sub" 2)
    (_     0)))          ; a type without variants needs `_`
```

Terhadap scrutinee `Sexpr`, selain 18 pola varian bawaan di atas, **pola downcast** (mengambil
instans ADT buatan pengguna) dapat ditulis: sintaks untuk mendapatkan kembali, dengan `match`, instans
`defstruct`/`defenum` (bab 3) yang dikonversi secara implisit menjadi `Sexpr`, seperti pada
`(list p 42)`:

- `(TypeName sub-pattern...)` — dekomposisi field dengan **nama tipe** di depan (hanya struct:
  `defstruct` selalu memiliki satu varian, sehingga ditulis dengan nama tipe dan bukan nama varian).
  Misalnya, untuk `(defstruct point (x f64) (y f64))`, `(point x y)`.
- Nama varian polos `(VariantName sub-pattern...)` — mengekstrak varian dari `defenum`. Diselesaikan
  sebagai nama polos yang terlihat setelah `(use EnumType)` (aturan keterlihatan yang sama seperti
  saat memanggil konstruktor). Misalnya, untuk `(defenum color (red) (blue))`, `(red)` `(blue)`
  setelah `(use color)`. Jika nama varian dari beberapa enum yang terlihat bentrok, itu adalah
  kesalahan ambiguitas, sehingga bentuk terkualifikasi `(EnumType::VariantName ...)` juga dapat
  ditulis (tanpa perlu `use`).
- `(the Type pattern)` — downcast seluruh tipe (mengikatnya secara utuh). Ia tidak mendekomposisi
  field; ia menyerahkan nilai ke `pattern` apa adanya. Satu-satunya cara mengambil struct mutable
  dengan mempertahankan identitasnya, dan juga satu-satunya cara mengambil
  `Vector<T>`/`HashTable<K,V>` dari `Sexpr` (keduanya tidak memiliki bentuk dekomposisi field).
  Misalnya, setelah `(the point p)`, `(setf p::x 9)` tercermin pada instans asli di dalam daftar juga.

**Pola untuk `Option<Sexpr>`**: tipe data S-expression bukan `Sexpr` melainkan `Option<Sexpr>`, dan
daftar kosong bukan varian `Sexpr` melainkan `none` pada `Option`. Jadi ketika mencocokkan
`Option<Sexpr>`, 18 varian `Sexpr` dan `none` dapat ditulis **rata dalam daftar cabang yang sama**
(tidak perlu `match` luar untuk mengupas `Option`):

```lisp
(defun tag ((s Option<Sexpr>)) i32
  (match s
    ((int _)    1)
    ((cons _ _) 2)
    ((str _)    3)
    ((none)     0)          ; the empty list
    (_          9)))
```

Kelengkapan diperiksa dalam semesta rata yang sama: 18 varian `Sexpr` ditambah `none`, 19 seluruhnya.
Melupakan `(none)` adalah kesalahan kecuali ada `_`. `(some x)` juga dapat ditulis dan mengikat
"sesuatu yang tak kosong".

Gula sintaks ini berlaku **tepat** hanya untuk `Option<Sexpr>`. Untuk `Option<Option<Sexpr>>`, tidak
jelas lapisan mana yang dikupas `(int n)`, jadi tulis dua tingkat `match` seperti biasa.

Pola downcast yang sama dapat dipakai apa adanya pada **scrutinee objek trait (`:dyn Trait`, bab 2)**:
`match` membuka kotaknya lalu menyerahkannya ke mesin pola `Sexpr` di atas, sehingga tidak ada
sintaks tambahan. Himpunan tipe pengimplementasi terbuka, sehingga tidak pernah dapat lengkap, dan
`_` diperlukan:

```lisp
(defun area ((d :dyn Drawable)) i32
  (match d
    ((circle r) (* (* r r) 3))     ; field decomposition with the type name first
    ((the square s) (* s::side s::side))
    (_ 0)))
```

**Inferensi tipe lintas cabang**: semua cabang harus bertipe sama (kecuali cabang yang diverge,
seperti dengan `panic`). Pada `match` yang ditulis di tempat tidak ada tipe yang diharapkan, cabang
saling mengisi argumen tipe yang hilang: `(result::ok v)` hanya menetapkan `T`, dan
`(result::err e)` hanya `E`, tetapi bersama-sama keduanya menetapkan `Result<T,E>`. Argumen tipe yang
tidak dapat ditetapkan cabang mana pun sampai akhir adalah kesalahan pada cabang itu
(`cannot infer type argument ...`). Di luar `match`, argumen tipe yang tidak dapat ditetapkan adalah
kesalahan saat itu juga.

Pemeriksaan kelengkapan `match` yang memakai pola downcast tidak menghitungnya sebagai cakupan varian
`Sexpr` sendiri (`match` yang hanya mendaftar pola downcast harus ditutup dengan `_`). Untuk ADT
generik (`defstruct point<T> ...` dan sebagainya), argumen tipe pada pola downcast tidak dapat
disimpulkan, sehingga bentuk dekomposisi field (`(point ...)`) dan bentuk nama varian polos tidak
dapat dipakai; nyatakan dengan `the`, seperti `(the point<i32> p)`.

**Downcast juga melihat instansiasi.** Argumen tipe eksplisit dipakai untuk pencocokan:
`(the point<i32> p)` hanya meloloskan nilai `point<i32>`, dan `point<string>` lewat ke cabang
berikutnya. Ini karena sebuah nilai mengingat tipenya termasuk argumen tipenya (mekanisme yang sama
yang memilih `print-object`).

```lisp
(if-let (pattern val) then els)     ; then (with bindings) if val matches pattern, else els. defmacro
(while-let (pattern val) body...)   ; loops while val (re-evaluated each time) matches pattern. defmacro
```

## 5. Iterasi

```lisp
(loop body...)                      ; an infinite loop. leave with break/return
(while test body...)                ; loops while test is true. defmacro
(until test body...)                ; loops while test is false (the negation of while). defmacro
(dotimes (var count-expr) body...)  ; evaluates count-expr once and runs var over 0..count-1. defmacro
(do ((var init step) ...)
    (test result...)
  body...)                          ; CL-style iteration with parallel stepping. defmacro
(do* ((var init step) ...)
     (test result...)
  body...)                          ; the sequential version of do (let* binding, assigned in order). defmacro
(doiter (var coll-expr) body...)    ; iterates over a value implementing the Iter trait. defmacro

(break)                             ; leaves only the innermost loop. the value is always Unit
(return)                            ; leaves only the innermost loop
(return value)                      ; leaves the innermost loop with a value
```

`break`/`return` keduanya meninggalkan **hanya perulangan terdalam yang melingkupinya** (keduanya
bukan return awal dari fungsi, dan tidak dapat melintasi batas `lambda`). Tipe `loop` adalah gabungan
tipe nilai dari `break`/`return` yang ada di dalamnya (`!` jika tidak pernah ditinggalkan). Untuk
meninggalkan fungsi, gunakan `return-from`, di bawah.

### 5.1 `block` / `return-from` — keluar bernama

```lisp
(block name body...)                ; a named exit target. the value is the last form,
                                    ; or the value passed by return-from
(return-from name)                  ; leaves that block with Unit
(return-from name value)            ; leaves with a value
```

**Setiap fungsi `defun` / `defmethod` / `labels` secara implisit membuat block dengan namanya
sendiri** (seperti di CL). Jadi `(return-from f v)` adalah return awal dari fungsi:

```lisp
(defun first-even ((a i32) (b i32)) i32
  (if (= (mod a 2) 0) (return-from first-even a) ())
  (if (= (mod b 2) 0) (return-from first-even b) ())
  -1)
```

`block` adalah keluar **leksikal**, dan nama **diselesaikan di tempat ia ditulis**: pemeriksa
mengasosiasikan `return-from` dengan `block` yang melingkupinya dan menggabungkan tipe nilainya ke
dalam tipe keluar block. Jadi:

- `return-from` tanpa `block` yang cocok adalah **kesalahan tipe** (bukan kesalahan saat dijalankan).
- Nilai yang tipenya tidak cocok dengan keluar lain atau tipe badan adalah **kesalahan tipe** (aturan
  yang sama seperti untuk cabang `match`).
- Jika block bernama sama bersarang, **yang dalam menang** (aturan bayangan CL).
- **Ia tidak dapat melintasi batas fungsi.** Dari dalam `lambda`, Anda tidak dapat keluar ke `block`
  di luar (`lambda` tidak membuat block: block implisit CL memerlukan *nama*, dan fungsi anonim tidak
  memilikinya). Yang perlu melintas adalah `catch`/`throw` (bab 8, yang **dinamis**).

Seperti `break`/`return` (bab 5), ia adalah keluar **statis**, sehingga pada kode terkompilasi ia
adalah percabangan ke basic block yang ditetapkan saat kompilasi. Jika ada `unwind-protect` di
antaranya, `cleanup`-nya dijalankan (bab 8).

Jika Anda tidak pernah menulis `return-from`, block implisit tidak memakan biaya.

### 5.2 `loop` yang diperluas (LOOP pada CL)

**Jika elemen pertama `loop` adalah keyword**, ia dibaca sebagai urutan klausa. Jika tidak, ia tetap
perulangan sederhana di atas, dan makna `loop` yang sudah ada tidak berubah (sama seperti aturan loop
sederhana CL sendiri).

CL menulis kata klausa sebagai simbol polos (`(loop for i from 1 to 3 collect i)`), tetapi di sini
**semuanya adalah keyword**: `for` polos hanyalah rujukan variabel, dan menjadi keyword juga yang
membedakannya dari loop sederhana. Pengecualiannya adalah `=`, yang memisahkan variabel dari nilai:
posisinya tidak ambigu, sehingga ia dibaca polos atau sebagai keyword (`:=`).

```lisp
(loop :for i :from 1 :to 3 :collect i)              ; #(1 2 3)
(loop :for x :in (iter v) :when (evenp x) :sum x)
(loop :repeat 4 :for x = 1 :then (* x 2) :collect x) ; #(1 2 4 8)
(loop :for i :from 1 :to 4 :sum i :into s :finally (return (* s 2))) ; 20
```

**Klausa variabel** (ditulis sebelum klausa badan. Ini aturan CL: jika ditulis sesudahnya, dapat
dibaca sebagai "iterasi hanya dari situ dan seterusnya", sehingga merupakan kesalahan):

| Klausa | Arti |
|---|---|
| `:with v = e` | Mengikat sekali. Boleh membaca variabel dari klausa sebelumnya |
| `:for v :in s` / `:for v :across s` | Elemen `Iter` secara berurutan. Pembedaan list/vector pada CL tidak ada di sini, sehingga ini dua ejaan dari klausa yang sama |
| `:for v :on s` | **Sufiks** yang berturut-turut. CL menyerahkan cons ekor yang dibagi, tetapi `Iter` tidak memiliki ekor untuk dibagi, sehingga masing-masing adalah `Vector` baru |
| `:for v :from a [:to b \| :below b \| :downto b \| :above b] [:by s]` | Menghitung. `:downfrom`/`:upfrom` juga berfungsi |
| `:for v = e [:then f]` | Dimulai dengan `e`, dan dari kali kedua memakai `f` (tanpa `:then`, `e` setiap kali) |
| `:repeat n` | Mengiterasi sebanyak itu |

Dengan beberapa `:for`, semuanya maju **secara paralel**, dan perulangan berakhir begitu salah satu
habis.

**Klausa badan** (dijalankan setiap kali, dalam urutan penulisan):

| Klausa | Arti |
|---|---|
| `:do form...` | Untuk efek samping |
| `:collect e [:into v]` | Mengumpulkan ke `Vector<T>` |
| `:append e [:into v]` | Menambahkan isi sebuah `Iter` |
| `:sum e` / `:count e` | Jumlah / banyaknya kali bernilai benar |
| `:maximize e` / `:minimize e` | Maksimum / minimum. **`Option<T>`** (seperti CL mengembalikan nil untuk sekuens kosong; tipe `Ord` sembarang tidak memiliki elemen terkecil) |
| `:always e` / `:never e` | `true` jika semuanya berlaku; `false` segera setelah satu gagal |
| `:thereis e` | `e` adalah **`Option<T>`**. Mengembalikan `some` pertama, atau `none` jika tidak ada (ini yang sepadan dengan "nilai non-nil pertama" pada CL; untuk menguji `bool`, gunakan `:always`/`:never`) |
| `:while e` / `:until e` | **Berakhir secara normal** di sini (`:finally` dijalankan, dan apa yang dikumpulkan adalah jawabannya) |
| `:when e clause` / `:unless e clause` / `:if e clause [:else clause]` | Menjadikan satu klausa bersyarat |
| `:return e` | Meninggalkan segera dengan nilai itu (`:finally` tidak dijalankan, seperti di CL) |
| `:initially form...` / `:finally form...` | Sebelum perulangan / saat selesai normal |

**`:named name`** (sebelum klausa lain mana pun, hanya sekali) membungkus seluruh perulangan dalam
`(block name …)`. `(return-from name e)` dapat meninggalkan sekaligus bahkan dari dalam perulangan
bersarang, dan seperti `:return`, `:finally` tidak dijalankan. Tanpa nama, tidak ada block yang dibuat:
`loop` tanpa nama pada CL membuat `block nil`, tetapi di sini tidak ada `nil`, dan `break`/`return`
(bab 5) sudah menyediakan "tinggalkan perulangan terdalam".

```lisp
(loop :named outer :for i :from 1 :to 3
  :do (loop :for j :from 1 :to 3 :do (if (= (* i j) 4) (return-from outer (* 100 i)) ()))
  :finally (return 0))                                  ; 200
```

Menghilangkan `:finally (return 0)` adalah **kesalahan tipe**. Ini hanyalah aturan `block` yang
bekerja (5.1): tipe keluar `int` tidak cocok dengan `()` yang ditinggalkan perulangan ketika habis.

**Nilai perulangan** adalah akumulasi dari klausa pengakumulasi jika ada (yang pertama, jika ada
beberapa), `true` untuk `:always`/`:never`, `none` untuk `:thereis`, dan `()` jika tidak ada. Jika
hal terakhir di `:finally` adalah `(return e)`, itulah nilainya: idiom `finally (return …)` pada CL,
satu-satunya cara perulangan yang tidak mengakumulasi dapat menamai jawabannya sendiri.

**Perbedaan dari CL / yang tidak disertakan**:

- **Kata klausa adalah keyword** (di atas).
- `:maximize`/`:minimize`/`:thereis` mengembalikan `Option<T>` (tidak ada nil).
- **Menulis hanya `:return`, tanpa akumulasi maupun `:finally`, adalah kesalahan.** CL mengembalikan
  nil ketika habis, tetapi di sini tidak ada hal seperti itu, sehingga perulangan harus menyatakan
  nilainya ketika habis.
- Menggabungkan klausa paralel dengan `:and`, `:being`/iterasi khusus atas tabel hash, `:it`, dan
  `:nconc` tidak disertakan.
- Tipe elemen `:collect` berasal dari tipe ekspresi yang diakumulasi. Mencoba mengumpulkan tipe yang
  **tidak dapat ditulis sebagai nama tipe**, seperti tipe fungsi, adalah kesalahan yang menyatakannya.

## 6. Nilai fungsi dan pemanggilan

```lisp
(lambda (params) RetType body...)   ; makes a first-class function value (a closure)
(labels ((name (params) RetType body...) ...) body...)   ; local function definitions that can be mutually recursive
(apply f arg1 ... argN rest-list)   ; calls f (a variadic function with &rest), spreading rest-list
```

Fungsi bernama juga dapat diserahkan sebagai nilai apa adanya (sebagai argumen fungsi tingkat tinggi
dan sebagainya).

## 7. Bentuk khusus lainnya

```lisp
(setq var value ...)                ; CL's variable assignment. just a sequence of (setf var value). defmacro
(psetq var value ...)               ; parallel assignment. evaluates all values first, then assigns. defmacro
(psetf place value ...)             ; psetq generalized to places (the same expansion). defmacro
(setf place value)                  ; assignment to a place. a place is a variable name / var::field /
                                     ; a call of the form (accessor recv key...). valid if the
                                     ; static type of recv has an instance method named
                                     ; set-{accessor} (for the get of Vector<T> and HashTable<K,V>,
                                     ; set corresponds as an exception; otherwise set-accessor-name).
                                     ; the value is the value assigned (as in CL). so
                                     ; in (if c (setf x 1) ()), then and else do not have matching types
(incf place)  (incf place delta)    ; place += delta (delta=1 if omitted). the result is as with setf
(decf place)  (decf place delta)    ; place -= delta (delta=1 if omitted)
(rotatef place1 place2 ... placeN)  ; rotates N places (new place1=old place2, ...,
                                     ; new placeN=old place1). each place's subforms evaluated once
(shiftf place1 ... placeN newvalue) ; shifts the values of place2..N left and puts newvalue in placeN.
                                     ; the return value is the old value of place1
(list e1 e2 ... en)                 ; expands to (cons e1 (cons e2 (... ()))). () with zero arguments.
                                     ; each element is converted to Sexpr implicitly (like CL's cons, it
                                     ; can hold any value). scalars (int/i32/f64/ratio/char/bool/string/
                                     ; symbol) are wrapped in the matching Sexpr variant, and defstruct/
                                     ; defenum/Vector<T>/HashTable<K,V> and the like go in as they are
                                     ; (at no conversion cost). the same for &rest/format arguments.
(source-file)                       ; the name of the file this form was read from (string). fixed as a
                                     ; constant at check time. corresponds to CL's *load-pathname*, but is
                                     ; not a variable: module bodies run after checking, so "currently
                                     ; loading" cannot be relied on, while at check time it is always known.
                                     ; for sources that are not files, the reader's name for them (<stdin>/<input>)
(quote datum)                       ; the same as 'datum. returns it as Sexpr data without evaluating
(quasiquote template)               ; the same as `template. embeds expressions in the template with ,/,@
(documentation name)                ; returns the docstring of name (a bare name or Type::method) as Option<string>
(panic message)                     ; message: string. ends abnormally with an unrecoverable error. type !
(unreachable)                       ; expands to (panic "unreachable"). defmacro
(todo)                              ; expands to (panic "todo"). defmacro
(as Type expr)                      ; numeric/character type conversion. conversions that can fail panic on failure
(try-as Type expr)                  ; like as, but returns the result as Option<Type> (None on failure)
(print control args...)             ; expands the format and writes to standard output (no newline)
(println control args...)           ; the same (with a newline at the end)
(format dest control args...)       ; CL's format. returns the expanded string
(pprint x)                          ; pretty-prints. writes a newline first, as in CL
(pprint-fill x)                     ; fill layout
(pprint-linear x)                   ; all on one line or one element per line
(pprint-tabular x [colinc])         ; tabular layout (16 columns by default)
(pprint-logical-block (obj :prefix p :suffix s) body...)  ; build a logical block yourself
```

Keluarga `print`/`println`/`format`/`pprint` adalah bentuk khusus, sehingga argumen variadiknya
(satu objek untuk keluarga `pprint`) dibungkus menjadi `Sexpr` dengan tipenya sendiri sebelum
diserahkan: inilah sebabnya `(println "~a" my-struct)` langsung bekerja. Rincian direktif format dan
pretty printer ada di [Direktif Format](functions/format.md) dan
[Pencetakan](functions/printing.md#4-pretty-printer).

`as`/`try-as` hanya menangani katalog numerik dan karakter (antara `int`, tipe bilangan bulat
berlebar tetap, `f32`/`f64`/`ratio`/`char`). Tipe yang sama bukan konversi. **Konversi antarlebar
bilangan bulat (termasuk `int`) dan antara `f32`↔`f64` adalah konversi sungguhan**: `as` memotong /
membulatkan, dan `try-as` menjawab apakah muat pada lebar (presisi) itu. `(as int x)` adalah pelebaran
tepat dari lebar tetap, dan `(as i32 n)` pemotongan dari `int`. Bilangan bulat → `char` dapat gagal di
luar rentang, sehingga `as` melakukan panic dan `try-as` menghasilkan `None`. Semua yang lain
(pelebaran, dan pemotongan `float->int`/`ratio->int`) selalu berhasil. `float->int`/`ratio->int`/
`char->int` mendarat pada `int`, dan jika lebar yang lebih sempit diminta, `int->W` dipanggil
setelahnya. Ini adalah gula sintaks yang diekspansi menjadi metode konversi yang sesuai
(`int->char`/`int->int`/`int->W` dan sebagainya di [Bilangan](functions/numbers.md)).

`documentation`, seperti `quote`/`compile`, adalah bentuk khusus yang membaca `name` tanpa
mengevaluasinya, sebagai simbol polos / path `::` yang tidak dievaluasi. Tidak seperti
`(documentation 'name 'function)` pada CL, ia tidak menerima argumen tipe: ia menyelesaikan `name`
dengan urutan variabel → fungsi → tipe → trait → makro (prioritas yang sama seperti ketika
pengenal polos dievaluasi sebagai ekspresi) dan mengembalikan docstring definisi yang ditemukan
(`(documentation Type::method)` untuk metode). Gagal menyelesaikan (tidak ada definisi bernama itu)
adalah kesalahan saat pemeriksaan; definisi yang ada tetapi tanpa docstring menghasilkan
`Option::none`. Semuanya ditentukan sebagai konstanta saat pemeriksaan: tidak ada pencarian saat
dijalankan. Nama bebas yang dikualifikasi modul (`mod::name`, kecuali `Type::method`) tidak didukung.

## 8. Keluar non-lokal (catch / throw / unwind-protect)

```lisp
(catch 'tag body)                   ; runs body. if (throw 'tag v) happens anywhere
                                    ; body reaches, that v becomes the value
(throw 'tag value)                  ; exits to the nearest dynamically enclosing (catch 'tag ...)
(unwind-protect protected cleanup)  ; runs cleanup however protected is left
```

Tidak seperti `break`/`return` (bab 5), ini adalah keluar **dinamis**: `throw` tidak mencari `catch`
di sekelilingnya secara leksikal, dan mencapai `catch` bertag sama melintasi sejumlah pemanggilan
fungsi.

```lisp
(defun find-first ((xs Option<Sexpr>)) int
  (catch 'found
    (progn
      (dolist (x xs)
        (match x ((int n) (if (> n 10) (throw 'found n) ())) (_ ())))
      -1)))                         ; if not found, the value at the end as usual
```

- **Tag hanya berupa simbol literal** (`'done`). Tidak seperti CL, tag tidak dievaluasi.
- **Tag membawa tipe.** Tipe ditentukan pertama kali `'tag` dipakai, dan setiap `throw`/`catch`
  berikutnya dengan simbol yang sama diperiksa terhadapnya. Memakainya dengan tipe lain adalah
  kesalahan tipe.
- Tipe `throw` adalah `!` (diverge). Tipe `(catch 'tag expr)` adalah gabungan tipe `expr` dan tipe
  tag.
- Nilai `unwind-protect` adalah nilai `protected`. Nilai `cleanup` dibuang. `cleanup` dijalankan
  bagaimanapun `protected` ditinggalkan: selain selesai normal, `throw`, dan `panic`, ia juga
  dijalankan ketika ditinggalkan oleh `break`/`return`/`return-from`. Keluar non-lokal oleh `cleanup`
  sendiri mengalahkan keluar yang sedang berlangsung.
- `unwind-protect` bersarang dijalankan dari dalam ke luar. `break` yang meninggalkan perulangan
  **di dalam** `protected` belum meninggalkan `protected`, sehingga `cleanup`-nya tidak dijalankan.

Kondisi CL (`define-condition`/`handler-bind`/`invoke-restart`) tidak diadopsi. Kondisi tidak cocok
dengan tipe statis, sehingga kegagalan yang dapat dipulihkan dinyatakan dengan `Result` (bab 9).

## 9. Kebijakan penanganan kesalahan

- Kegagalan yang dapat dipulihkan: `Result<T,E>` + `match`. Kegagalan yang tidak dapat dipulihkan
  (bug, invarian yang rusak): `panic`.
- Tidak ada sintaks yang sepadan dengan `?`/try. Percabangan ditulis secara eksplisit dengan `match`.
- Nama fungsi dan bentuk khusus tidak memakai `!` (operasi destruktif) atau `?` (predikat) sebagai
  akhiran. Predikat dinamai dengan akhiran `-p`/`p` (`zerop`, `consp`, dan sebagainya) atau awalan
  `is-` (`is-some`, `is-ok`, dan sebagainya).

## 10. Kompilasi

```lisp
(compile name)                      ; JIT-compiles an already defined defun/method into native code
(compile-file src-path out-path)    ; AOT-compiles a source file into a native executable (skips the final `(main)`)
(dump path)                         ; writes the current environment (type information + compiled bodies) to one file
(disassemble name)                  ; prints what that definition becomes (host machine code by default, LLVM IR with true as the second argument)
```

`compile` adalah bentuk khusus; `name` tidak dievaluasi dan dibaca sebagai simbol polos / path `::`
yang tidak dievaluasi (string adalah kesalahan tipe). Fungsi generik tidak dapat menjadi sasaran:
salinan untuk setiap tipe dibuat di tiap tempat pemakaian, sehingga tidak ada satu badan terkompilasi.
**Nama yang tidak dapat diselesaikan adalah kesalahan saat pemeriksaan** dan tidak pernah dibawa ke
saat dijalankan (ada pesan terpisah untuk: tipe ada tetapi metode itu tidak / baik tipe maupun fungsi
tidak ada / nama polos yang tidak terdefinisi). Keterlihatan di sini diperlakukan seperti untuk
rujukan lain: "ada tetapi tidak terlihat dari sini" gagal saat pemeriksaan, sama seperti "tidak dapat
diselesaikan".

Callee juga dikompilasi secara transitif, sehingga **fungsi yang (bahkan secara tidak langsung)
memanggil sesuatu yang tidak dapat dikompilasi tidak dapat dikompilasi**. Proses tidak crash; ia
ditolak dengan kesalahan yang menyatakannya. Setiap fungsi bawaan dapat dikompilasi, sehingga satu-satunya
fungsi yang ditolak dengan cara ini adalah yang memanggil operasi khusus interpreter berikut:

```lisp
(defun g () int 1)
(defun f () () (progn (trace g) ()))
(compile f)
; => trace: `(trace ...)` is an interpreter-only action and cannot itself be compiled
```

Yang khusus interpreter adalah `compile`/`compile-file`/`dump` dan
`trace`/`untrace`/`step`/`disassemble`
([Alat implementasi](functions/system.md#5-alat-implementasi-clhs-252)). Daripada hal yang tidak dapat
dikompilasi, ini adalah operasi dari sisi yang mengompilasi (yang ditulis `dump` adalah lingkungan
interpreter itu sendiri, yang tidak dimiliki berkas eksekusi AOT; yang diawasi `trace` dan tempat
`step` berhenti adalah jalur pemanggilan interpreter yang berjalan; dan `disassemble` memakai kompiler
itu sendiri). `room`/`dribble`/`ed` bukan bagian dari ini dan dapat dikompilasi secara normal.

Yang **dapat** dikompilasi: I/O stream dan berkas, `random`, `gensym`,
`symbol->string`/`string->symbol`, `parse-int`/`parse-float`,
`get-universal-time`/`get-internal-real-time`, `exit`, fungsi transendental, operasi bit,
`catch`/`throw`/`unwind-protect`, keempat `eq`/`eql`/`equal`/`equalp` (yang membuat `case` dapat
dikompilasi untuk setiap tipe), seluruh keluarga pencetakan termasuk
`print`/`println`/`format`/`pprint` dan `pprint-logical-block`, `read`, dan `eval`. Pustaka standar
dikirim sudah terkompilasi.

Berkas eksekusi AOT hanya berisi fitur yang dipakai program. Program yang tidak mencetak tidak
mendapat mesin pemformatan, yang tidak memanggil `read` tidak mendapat reader, dan yang tidak
memanggil `eval` tidak mendapat pemeriksa maupun interpreter.

Dari baris perintah, `typl -c src-path [-o out-path]` (`-c` juga dapat ditulis `--compile`)
melakukan hal yang sama seperti `compile-file`. Tanpa `-o`, keluarannya adalah `src-path` dengan
ekstensi `.typl` dihapus. Secara bawaan, pustaka statis `libtypelisp_front.a` yang ditautkan ke
berkas eksekusi adalah, untuk build rilis `typl`, yang dibawa `typl` di dalam dirinya, ditulis keluar
pada penautan pertama ke `$TYPELISP_HOME/lib/<ID build>/` (atau `~/.typelisp/lib/<ID build>/` tanpa
`TYPELISP_HOME`) dan dipakai dari sana; untuk build debug, yang ada di tempat `typl` dibangun.
`typl --remove-lib` menghapus apa yang ditulis `typl` itu. Dengan `--others`, ia menghapus yang
berasal dari ID build lain; dengan `--all`, yang berasal dari setiap ID build. Dengan
`typl --lib-dir DIR`, yang ada di `DIR` dipakai (baik untuk `-c` maupun `compile-file`), dan jika
tidak ada di sana, itu adalah kesalahan saat start.

### 10.1 Dump

```lisp
(dump "session.typld")     ; write one out
```
```sh
typl --image session.typld prog.typl   # start from it
typl --image session.typld             # the REPL too
```

Dump menyimpan informasi tipe dan badan terkompilasi dalam satu berkas. Yang ditulis `(dump path)`
adalah apa yang dimuat sesi saat ini (pustaka standar, atau dump yang diserahkan dengan `--image`)
ditambah **apa yang didefinisikan sesi itu sendiri**. Jadi keluarannya berdiri sendiri, dan
`typl --image` memunculkan lingkungan yang sama. Apa yang di-`(compile f)` oleh sesi ditulis dalam
bentuk terkompilasinya.

Yang disimpan adalah **definisi, bukan riwayat**:

- Ekspresi tingkat atas sesi (`(println ...)` dan sebagainya) tidak disertakan. Akan menjadi masalah
  jika pemuatan menjalankannya ulang.
- Variabel global kembali dengan **nilai penginisialisasinya yang dijalankan ulang**, bukan nilai
  saat dump. Ini perbedaan yang disengaja dari `save-lisp-and-die` pada SBCL (yang menulis heap apa
  adanya), dan pilihan ini menghilangkan satu keluarga masalah: "nilai yang tidak dapat disimpan",
  seperti stream yang terbuka, pointer fungsi closure, dan memori eksternal.
- Tidak seperti `save-lisp-and-die`, **proses tidak mati**, karena penulisan tidak merusak image.

Dump mencatat versi pustaka standar dan kompiler implementasi yang menulisnya. Memuatnya dengan
`typl` berversi berbeda adalah kesalahan; ia tidak pernah diterima secara diam-diam.

### 10.2 `eval` pada berkas eksekusi AOT

`eval` memeriksa tipe terhadap "lingkungan global saat ini" lalu mengevaluasi
([Parsing dan evaluasi](functions/system.md#6-parsing-dan-evaluasi)). Lingkungan itu (tabel
signature, tipe, dan makro yang dirujuk pemeriksa, dan badan yang dapat dijalankan interpreter)
**tidak ada di dalam kode mesin**. Fungsi terkompilasi tidak lain adalah simbol yang ditaruh pada
sebuah alamat; ia tidak memiliki tipe argumennya maupun tabel untuk mencari badan berdasarkan nama.

Jadi, hanya untuk program yang memanggil `eval`, `compile-file` **membangun lingkungan itu saat
kompilasi dan menuliskannya ke dalam berkas eksekusi**. Formatnya sama seperti dump, berisi bagian
pustaka standar dan bagian program itu sendiri. Yang terjadi saat start hanyalah memulihkannya:
sumber tidak dibaca ulang, dan tidak ada yang diperiksa tipenya lagi. Tidak ada yang ditambahkan ke
program yang tidak memanggil `eval`.

Akibatnya:

- **Start lebih lama dan berkas eksekusi lebih besar**, karena kode pemeriksa dan interpreter serta
  snapshot lingkungan masuk. Heap juga dibuat sedikit lebih besar.
- **Bentuk yang diserahkan ke eval diinterpretasi.** Bahkan ketika bentuk yang diserahkan ke eval
  memanggil fungsi program itu sendiri, yang berjalan adalah badan yang dapat diinterpretasi yang
  dimiliki snapshot. Hasilnya sama; hanya kecepatannya yang berbeda.

Penyimpanan variabel global **dibagi** dengan kode terkompilasi (slot yang sama). Penginisialisasi
`defvar` dijalankan sekali oleh inisialisasi terkompilasi, dan pemulihan melewatinya, sehingga
penginisialisasi dengan efek samping tidak berjalan dua kali.

`compile-file` juga membaca pustaka standar (dan menyematkan badannya ke dalam berkas eksekusi),
sehingga fungsi pustaka standar seperti `abs`/`gcd`, serta `(impl print-object ...)` maupun
`(defmethod print-object ...)`, dapat dipakai dengan AOT.

`compile-file` juga menerima `use` (dan `import`/`shadowing-import`). `(use m)` pada berkas masuk
menemukan berkas dengan aturan yang sama seperti `typl file.typl`, dan berkas dependensi yang
ditemukan juga dikompilasi dan ditautkan ke dalam berkas eksekusi: tata letak tempat `main.typl`
membaca `http.typl` melalui `(use http)` dapat dikompilasi AOT apa adanya. Definisi berkas masuk itu
sendiri juga masuk ke modul yang dinamai sesuai berkas, seperti pada `typl file.typl` (`point` pada
`p.typl` adalah `p::point`). Jadi representasi cetak nilai (`#<p::point x: 1 y: 2>`) sama bagaimanapun
cara menjalankannya.

## 11. Reader macro (readtable)

Apa yang **dilakukan reader ketika bertemu karakter tertentu** dapat diganti dari program (CLHS 23.1).

```lisp
(set-macro-character c f)             ; f reads the character c
(get-macro-character c)               ; Option<f>
(set-dispatch-macro-character d s f)  ; f reads the two-character sequence d s
(get-dispatch-macro-character d s)    ; Option<f>
```

Tipe `f` adalah `(fn (string-input-stream char) Option<Sexpr>)`. Argumen pertama adalah **stream atas
teks yang belum dibaca**, dan yang kedua adalah **karakter yang memicunya** (karakter kedua untuk
dispatch). Nilai kembalian menjadi data yang dibaca pada titik itu. Stream adalah tipe konkret dan
bukan `:dyn PeekInput` karena reader selalu menyerahkan satu jenis ini: `read-sexpr` / `read-char` /
`peek-char` / `unread-char` / `read-delimited-list` semuanya menerima `(where (PeekInput S))`,
sehingga semuanya bekerja pada tipe konkret apa adanya.

```lisp
(set-macro-character #\!
  (lambda ((s string-input-stream) (c char)) Option<Sexpr>
    (match (read-sexpr s)
      ((ok o) (match o
                ((datum d) (sexpr-cons (quote not) (sexpr-cons d (quote ()))))
                ((eof) (quote ()))))
      ((err e) (quote ())))))

!(equal 1 2)   ; => read as (not (equal 1 2)), that is, true
```

Reader **melihat karakter makro sebelum sintaks bawaan**, sehingga ia dapat mengambil alih `(` dan
`'` juga. Sub-karakter `#` yang didaftarkan dengan cara ini diutamakan di atas `#b`/`#x`/`#.` bawaan.
Karakter selain `#` menjadi karakter dispatching saat itu juga ketika diserahkan ke
`set-dispatch-macro-character`: **tidak ada** padanan `make-dispatch-macro-character` pada CL.
Pendaftaran sudah melakukan tugasnya, sehingga langkah terpisah tidak akan memiliki pekerjaan.

**Kapan berlaku** bergantung pada jalur pembacaan, sama seperti `#.` (bab 1):

- REPL dan `(load ...)` menjalankan satu bentuk sekali waktu, sehingga **fungsi yang didefinisikan
  pada bentuk sebelumnya** dapat didaftarkan apa adanya.
- Berkas modul diperiksa sebagai satu kesatuan dan dijalankan kemudian, sehingga **hanya pemanggilan
  `set-macro-character` / `set-dispatch-macro-character` yang langsung dijalankan** (peran
  `(eval-when (:compile-toplevel) ...)` pada CL). Karena dijalankan langsung, **fungsi yang diserahkan
  harus sudah ada pada titik itu**. `defun` di berkas yang sama belum dijalankan, jadi tulis
  `lambda`, atau pakai pustaka standar atau sesuatu yang sudah dijalankan. Hanya pemanggilan tingkat
  atas yang tercakup; ia tidak melihat ke dalam `progn` atau `let`.

`read` / `read-from-string` bawaan juga merujuk readtable (seperti di CL).

**Yang tidak ada**: `*readtable*` dan `copy-readtable`, serta `readtable-case`. Dua yang pertama
karena readtable **bukan sebuah nilai**: nilai harus "sesuatu yang dapat diserahkan ke reader", tetapi
reader yang membaca sumber berada di luar program, tanpa tempat untuk menyerahkannya.
`readtable-case` karena bab 1 menetapkan bahwa reader bahasa ini selalu mengubah ke huruf kecil
(`:downcase` pada CL).


## 12. Konkurensi (task)

**Task adalah thread ringan** (dalam istilah Go, yang dimulai oleh pernyataan `go`) dan berjalan
secara kooperatif (tidak ada preemption). Perpindahan tidak melalui kernel, dan keadaan eksekusi
berada di heap dan bukan di stack mesin, sehingga task murah dibuat dalam jumlah besar.

**Task berjalan pada saat yang sama di beberapa thread OS** (paralelisme multi-core). Jumlah thread
adalah variabel lingkungan `TYPELISP_THREADS` (total, termasuk thread yang menjalankan `main`; bawaannya
adalah paralelisme mesin). Di `typl`, **hanya task terkompilasi** yang berjalan di thread lain, dan
task yang diinterpretasi berjalan di thread interpreter (12.7). Data bersama melalui `Mutex<T>` atau
`Chan<T>`; pembacaan dan penulisan serentak yang tidak melalui keduanya tidak terdefinisi, seperti di
Go (12.7).

Dari kosakata ini, **hanya `task` / `thread` / `select` yang merupakan bentuk khusus**; sisanya adalah
fungsi, metode, dan makro biasa ([Task dan Kanal](functions/concurrency.md)).

### 12.1 `task` — memulai task

```lisp
(task (f arg...))                   ; returns Task<T>, where T is the return type of f
```

**Ia hanya menerima bentuk pemanggilan.** `f` dan setiap `arg` dievaluasi di tempat `task` ditulis,
dalam urutan penulisan, dan hanya **pemanggilannya** yang terjadi di task baru. Ini aturan yang sama
seperti `go f(x)` pada Go, dan juga alasan ia menerima bentuk pemanggilan dan bukan thunk: thunk akan
menangkap argumennya tanpa mengevaluasinya.

```lisp
(dotimes (i 10)
  (task (worker i ch)))             ; i is evaluated on the spot each time; no capture trap

(task ((lambda () ()                ; to run an arbitrary body, call a lambda
         (println "start")
         (send ch 1))))
```

Bentuk khusus (`if` / `let` / `progn` …) tidak dapat ditulis langsung di bawah `task`.

**Mengapa ia tidak dapat menjadi fungsi**: menulis `(spawn (lambda () T body...))` akan mengharuskan
mengeja `T`, karena `lambda` memerlukan anotasi tipe kembalian, dan makro tidak mengetahui tipe
kembalian `(f a b)`. Hanya pemeriksa yang mengetahuinya.

### 12.2 `thread` — memulai task pada thread OS khusus

```lisp
(thread (f arg...))                 ; returns Thread<T>, where T is the return type of f
(join th)                           ; waits for completion and returns its value (any number of times)
```

Bentuk dan aturan evaluasinya sama seperti `task` (hanya menerima bentuk pemanggilan, dan `f` serta
`arg` dievaluasi di tempat ia ditulis). Bedanya adalah tempat ia berjalan: **ia memulai thread OS
khusus untuk task itu dan berjalan hanya di sana**. Ia tidak dimultipleks dengan task lain, sehingga
memanggil fungsi C yang memblokir (`defffi`) di dalamnya hanya menghentikan thread itu, dan task lain
tetap maju. Di dalamnya, `task`, `send`, `recv`, dan sisanya dapat dipakai apa adanya.

- `Thread<T>` adalah padanan `Task<T>`. Seperti `wait`, `join` menghentikan **task pemanggil**, dan
  nilainya di-cache. Ketika task berakhir, thread juga berakhir.
- Aturan panic sama seperti untuk `task` (seluruh proses tumbang). Ketika `main` kembali, proses
  berakhir.
- Untuk menuliskannya sebagai fungsi, gunakan `(Thread::spawn (lambda () T body...))`
  (`std::thread::spawn` pada Rust). Fungsi bernama juga boleh diserahkan.
- **Hanya kode terkompilasi yang berjalan di thread khusus.** Ketika `typl` mengevaluasi
  `(thread (f ...))` atau `Thread::spawn` saat menginterpretasi, ia mengompilasi fungsi yang akan
  dijalankan (dan yang dipanggilnya) saat itu juga sebelum menjalankannya. Yang tidak dapat
  dikompilasi (`lambda` yang merujuk variabel lokal di luarnya, membangun struct, dan sebagainya)
  adalah, sebelum thread dimulai, panic yang diperlakukan sama seperti `(panic ...)`. `lambda` yang
  merujuk variabel lokal dapat diserahkan jika dibuat di dalam fungsi terkompilasi.

### 12.3 `select` — menunggu beberapa operasi kanal sekaligus

```lisp
(select
  ((v (recv ch1)) body...)          ; a receive arm. v is bound to an Option<T>
  ((send ch2 x) body...)            ; a send arm
  (else body...))                   ; optional. **if written, it goes last**
```

- **Dengan `else`, ia tidak memblokir** (`default` pada Go). Tanpanya, ia menunggu sampai salah satu
  menjadi mungkin.
- **Jika beberapa mungkin sekaligus, satu dipilih secara acak** (jika berdasarkan urutan penulisan,
  cabang belakangan akan kelaparan).
- `v` pada cabang penerimaan adalah **`Option<T>`**. Kanal yang ditutup adalah "sebuah jawaban",
  bukan alasan melewati cabang, sehingga `match` padanya di dalam cabang.
- Tipenya adalah **gabungan tipe badan semua cabang** (aturan yang sama seperti cabang `match`).
- `(select)` dengan nol cabang adalah kesalahan tipe (`select{}` pada Go, memblokir selamanya, tidak
  diadopsi). `select` yang hanya berisi `else` juga, karena sama dengan menulis badan langsung.

**Ekspresi kanal dan nilai yang dikirim dievaluasi sekali masing-masing, dari kiri ke kanan, cabang
mana pun yang dipilih** (disiplin yang sama dimiliki `case` untuk kuncinya).

```lisp
(select                             ; receiving with a timeout
  ((v (recv ch))          (println "~a" (unwrap v)))
  ((z (recv (after 0.5))) (println "timeout")))
```

`after` ([kanal yang mengirim setelah suatu waktu](functions/concurrency.md#5-after--kanal-yang-mengirim-setelah-suatu-waktu))
adalah "kanal yang mengirim satu nilai setelah `sec` detik", sepadan dengan `time.After` pada Go.

### 12.4 Interaksi dengan fitur lain

| Fitur | Hubungannya dengan task |
|---|---|
| `catch` / `throw` | **Tidak melintasi batas task.** `throw` yang mencoba meninggalkan badan task adalah panic |
| `unwind-protect` | Pembersihan dijalankan ketika task berakhir secara alami. **Tidak dijalankan ketika proses berakhir karena task utama berakhir** |
| `block` / `return-from` | Leksikal, sehingga tidak melintasi batas `lambda` |
| `panic` | Seperti di Go, seluruh proses tumbang. `wait` tidak mengamati panic sebagai nilai |
| `dlet` | **Bukan pengikatan per task.** Ia tetap "meminjam dan mengembalikan variabel global", sehingga task saling mengganggu |
| Keluaran standar | Dibagi oleh semua task. Keluaran satu `println` tidak pernah bercampur dengan yang lain di tengah baris |
| `compile` / `eval` | Tanpa batasan. `(compile f)` di dalam task berfungsi |

### 12.5 Tempat task berpindah

Penjadwalan bersifat kooperatif, sehingga **task berpindah hanya di tempat Anda menuliskannya**:
`(yield)`, `(sleep ...)`, `(wait ...)`, **operasi kanal yang harus menunggu**
(`send`/`recv`/`select`), dan **operasi socket yang harus menunggu** (`accept` / `tcp-connect`
(termasuk resolusi nama) / membaca dan menulis socket / `recv-from`;
[Jaringan](functions/network.md)). Semua socket bersifat non-blocking: jika belum siap, hanya task itu
yang berhenti, dan ia dilanjutkan ketika OS menyatakan siap, bentuk yang sama seperti netpoller Go.
Hanya ketika tidak ada task yang dapat berjalan, implementasi menunggu OS sampai tenggat `sleep`
terdekat.

Operasi kanal yang dapat menjawab saat itu juga (`send` dengan ruang di buffer, `recv` dengan nilai
yang menunggu, `(len ch)`/`(cap ch)`/`(close ch)`/`(Chan::new n)`) **tidak menghabiskan giliran**.
Ini berarti Anda tidak terinterupsi tak terduga oleh pembacaan, dan diperlakukan berbeda dari
`(sleep 0.0)`, yang merupakan "yield selama 0 detik" pada CL.

**Tidak ada preemption.** Perulangan ketat yang tidak memanggil apa pun membuat task lain kelaparan.
Namun, perulangan terkompilasi menyerahkan kendali ke scheduler secara berkala, sehingga perulangan
ketat terkompilasi tidak membuat mereka kelaparan.

### 12.6 Kode terkompilasi dan task

Kode terkompilasi juga dapat menangguhkan task. Hal yang sama berlaku untuk berkas eksekusi yang
dibuat dengan `compile-file`: `main` berjalan sebagai task utama scheduler, dan `task`, `sleep`,
`wait`, kanal, dan penantian socket semuanya bekerja dengan makna yang sama seperti di `typl`.
Ketika `main` kembali, proses berakhir dan task yang tersisa dihentikan paksa (seperti di Go).
Interpreter tidak pernah dimasukkan ke dalam berkas eksekusi demi scheduler.

Satu-satunya pengecualian adalah "di dalam callback FFI C", tempat operasi yang **harus menunggu**
adalah kesalahan (lebih ramah daripada deadlock tanpa suara): selama fungsi yang diserahkan dengan
`defffi` dipanggil dari C, stack C berada di atas, dan tidak ada cara menangguhkan task lalu
melanjutkannya kemudian.

Tempat-tempat berikut juga adalah fungsi yang dipanggil di tengah task, tetapi tidak dapat
menangguhkan: metode `print-object`, `~/name/` pada `format`, reader macro, di dalam `eval`, dan
penginisialisasi `defvar` pada berkas eksekusi AOT. Di sini, **operasi yang menjawab tanpa menunggu
diloloskan** (`(recv ch)` dengan nilai di buffer, `read-line` pada socket dengan data yang sudah
diterima, `(task ...)`, `(yield)`, dan sebagainya), dan **operasi yang benar-benar harus menunggu
adalah kesalahan** (bukan menghentikan proses saat itu juga, tetapi panic seperti
`` `recv` cannot block: ... ``, diperlakukan sama seperti `(panic ...)`).

### 12.7 Perbedaan dengan Go

- **Di `typl`, hanya task terkompilasi yang keluar ke thread lain.** Keadaan interpreter tidak dapat
  dibagi antarthread, sehingga task dari `task` yang diinterpretasi berjalan di thread interpreter.
  Task terkompilasi pun **pindah ke thread interpreter dan tetap di sana** (tidak kembali) pada titik
  ia memanggil nilai fungsi yang diinterpretasi, memanggil metode `:dyn` yang tidak dikompilasi siapa
  pun, atau memanggil `eval`/`macroexpand`/`read`. Jika komputasi panjang menyentuh kode yang
  diinterpretasi bahkan sekali di tengah jalan, sisanya berjalan di thread interpreter.
- **Di `typl`, worker hidup hanya untuk satu evaluasi tingkat atas.** Ketika REPL menunggu masukan,
  dan di antara bentuk tingkat atas, thread lain tidak memajukan task (task yang tersisa berlanjut
  dari tempat berhenti pada evaluasi berikutnya). Di akhir evaluasi, ia menunggu setiap thread
  menyelesaikan langkahnya saat ini, sehingga jika fungsi C (`defffi`) terus memblokir di dalam
  `thread`, evaluasi tidak berakhir sampai ia kembali.
- **Pencetakan pada worker**: metode `print-object` / `~/name/` yang diinterpretasi tidak dapat
  berjalan di thread lain, sehingga mencetak nilai seperti itu di thread lain adalah panic yang
  diperlakukan sama seperti `(panic ...)` (`(compile T::print-object)`, atau cetak dari task utama).
- **Data race tidak terdefinisi** (posisi yang sama seperti Go). Hasil beberapa task yang mengubah
  nilai yang sama tanpa melalui `Mutex<T>` / `Chan<T>` tidak dijamin.
- **`task` mengembalikan nilai.** Tidak seperti pernyataan `go` pada Go, ia mengembalikan `Task<T>`,
  dan `(wait t)` mengambil hasilnya.
- **Tidak ada kanal nil.** Idiom fan-in Go (menyetel kanal yang ditutup ke `nil` untuk
  mengeluarkannya dari cabang `select`) tidak dapat ditulis, sehingga mulai satu task per masukan dan
  gabungkan dengan `WaitGroup`
  ([WaitGroup](functions/concurrency.md#4-waitgroup--menunggu-n-penyelesaian)). Itu juga cara yang
  direkomendasikan di Go, tetapi ia adalah **perbedaan pertama yang ditemui orang yang datang dari
  Go**.
