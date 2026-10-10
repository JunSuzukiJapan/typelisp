<!-- translated-from: docs/ja/reference/functions/option-result.md @ 7bc227fccd76e389ecb46e23b24421514247c86c -->
# Option, Result och feltyper

## 1. `Option<T>` / `Result<T,E>`

Konstruktorer: `Option<T>` har `Some(T)` / `None`. `Result<T,E>` har `Ok(T)` / `Err(E)`. `E` kan vara
vilken typ som helst: de inbyggda konkreta feltyperna, och typer du skriver själv med
`defstruct`/`defenum`, passar där lika bra (kapitel 3).

| Namn | Form | Option | Result | Beskrivning |
|---|---|---|---|---|
| `unwrap` | `(unwrap self)` | `Option<T>→T` | `Result<T,E>→T` | Tar ut värdet. Ger panic vid `None`/`Err` |
| `unwrap-or` | `(unwrap-or self default)` | `(Option<T>,T)→T` | `(Result<T,E>,T)→T` | Värdet, eller standardvärdet |
| `is-some` | `(is-some self)` | `Option<T>→bool` | — | Om det är `Some` |
| `is-none` | `(is-none self)` | `Option<T>→bool` | — | Om det är `None` |
| `is-ok` | `(is-ok self)` | — | `Result<T,E>→bool` | Om det är `Ok` |
| `is-err` | `(is-err self)` | — | `Result<T,E>→bool` | Om det är `Err` |
| `expect` | `(expect self msg)` | `(Option<T>,string)→T` | `(Result<T,E>,string)→T` | Tar ut värdet. Vid `None`/`Err` blir det panic med `msg` |
| `unwrap-or-else` | `(unwrap-or-else self f)` | `(Option<T>,fn()→T)→T` | `(Result<T,E>,fn(E)→T)→T` | Värdet, eller resultatet av `f`. `f` anropas bara vid `None`/`Err` |
| `map` | `(map self f)` | `(Option<T>,fn(T)→U)→Option<U>` | `(Result<T,E>,fn(T)→U)→Result<U,E>` | Tillämpar `f` på innehållet i `Some`/`Ok` |
| `map-err` | `(map-err self f)` | — | `(Result<T,E>,fn(E)→F)→Result<T,F>` | Tillämpar `f` på innehållet i `Err` |
| `and-then` | `(and-then self f)` | `(Option<T>,fn(T)→Option<U>)→Option<U>` | `(Result<T,E>,fn(T)→Result<U,E>)→Result<U,E>` | Vid `Some`/`Ok` skickas innehållet till `f` och dess resultat returneras |
| `or-else` | `(or-else self f)` | `(Option<T>,fn()→Option<T>)→Option<T>` | `(Result<T,E>,fn(E)→Result<T,F>)→Result<T,F>` | Vid `None`/`Err` returneras resultatet av `f` |
| `ok-or` | `(ok-or self e)` | `(Option<T>,E)→Result<T,E>` | — | Gör om `Some(v)` till `Ok(v)` och `None` till `Err(e)` |

Konstruktorerna är `Option::some`/`Option::none`/`Result::ok`/`Result::err` (eller, efter
`(use option)`/`(use result)`, de bara namnen `some`/`none`/`ok`/`err`).

Förgreningar skrivs uttryckligen med `match`, eller kedjas med `map`/`and-then` och de andra ovan.
Det finns ingen syntax som motsvarar Rusts `?`.

`map` för `Option`/`Result` är en metod, skild från `map` för [sekvenser](sequences.md). Den anropas
när det första argumentets typ är `Option`/`Result`.

Makrot `->` skickar ett värde som första argument till varje följande form i tur och ordning (som
Clojures `->`). `(-> x (f a) (g b))` blir `(g (f x a) b)`. Ett namn utan parenteser, `h`, behandlas
som `(h x)`. En metods första argument är dess mottagare, så kombinatorerna kedjas som de är:

```lisp
(defun half ((n int)) Option<int>
  (if (= 0 (mod n 2)) (option::some (/ n 2)) (option::none)))

(-> (option::some 8)
    (and-then half)                          ; (some 4)
    (and-then half)                          ; (some 2)
    (map (lambda ((x int)) int (* x 10)))    ; (some 20)
    (unwrap-or 0))                           ; => 20

(-> (parse-int "x")
    (map-err (lambda ((e ParseIntError)) string (message e)))
    (unwrap-or-else (lambda ((m string)) int (length m))))
```

## 2. Körtidsrepresentationen av `Option<T>`

Som i Rust **gör `Option<T>` vanligtvis ingen ruta (box)**. `some v` är `v` självt och `none` är
värdet för den tomma listan, utan allokering och utan indirektion. `Option<Sexpr>` (där den tomma listan
är `none`), `Option<int>`, `Option<string>`, `Option<my-struct>`, `Option<f64>` och `Option<(fn ...)>`
har alla den här formen.

En ruta används bara när ett värde av typen `T` inte kan skiljas från värdet för den tomma listan:

| `T` | Representation | Skäl |
|---|---|---|
| `Option<U>` (nästlad) | Ruta | Det inre `none` skulle vara samma värde som det yttre `none` |
| `()` | Ruta | Värdet av `()` är själva värdet för den tomma listan |
| `ptr` / `c-long` / `c-ulong` | Ruta | Alla 64 bitar är värde, vilket inte lämnar något utrymme att skilja dem åt |
| Allt annat | Ingen ruta | — |

Representationen avgörs av typen ensam och kan inte läsas från ett värde. Vid utskrift rekonstrueras
`(some ...)`/`none` från den statiska typen, så `(format false "~a" opt)` skriver ut `(some 1)`. Det finns
två begränsningar:

- **Det kan inte läggas i en `:dyn Trait`** (att skicka ett värde av `Option<int>` för vilket du skrivit
  `(impl Speak Option<int> ...)` till en `:dyn Speak` är ett fel).
- En nedkastning `(the Option<T> ...)` från en `Sexpr` **namnger en konstruktor**:
  `(the Option<int> (some x))` / `(the Option<int> (none))`. Formen som binder hela värdet,
  `(the Option<int> o)`, är ett fel.

## 3. Feltyper och traitet `Error`

Efter Rusts `std::error::Error` är **`Error` inte en typ utan ett trait**. De konkreta typer som
representerar fel är separata för varje syfte, och var och en implementerar `Error`.

| Typ | Skapas av |
|---|---|
| `ParseIntError` | `parse-int` |
| `ParseFloatError` | `parse-float` |
| `ReadError` | `read` / `read-from-string` / `read-sexpr` |
| `EvalError` | `eval` / `macroexpand` |
| `FileError` | Fil- och strömoperationer ([Strömmar och filer](streams-files.md)) |
| `NetError` | Nätverksoperationer ([Nätverk](network.md)) |
| `SimpleError` | `(SimpleError::new msg)` / `(simple-error msg)`. CL:s `simple-error`: standardvalet när du bara vill säga vad som hände |
| `WrappedError` | `(wrap-error msg cause)`. En typ som bär både ditt eget meddelande och orsaken; det är därför traitet `Error` har `source` |

`ParseIntError` till `NetError` är var och en "en enum med en enda variant som håller en
meddelandesträng", och typnamnet och variantnamnet är desamma (`(match e ((ParseIntError m) m))`, byggs
med `(ParseIntError::ParseIntError "...")`). Det är inget speciellt med dem: de behandlas precis som
dina egna feltyper skrivna med `(defstruct my-err (...))` / `(defenum my-err ...)`.

| Namn | Form | Typ | Beskrivning |
|---|---|---|---|
| `message` | `(message e)` | `Self→string` | Felmeddelandet (en metod i traitet `Error`) |
| `source` | `(source e)` | `Self→Option<:dyn Error>` | Orsaken som det här felet omsluter, eller `None` om det inte finns någon (Rusts `Error::source`) |
| `as-dyn-error` | `(as-dyn-error r)` | `Result<T,E>→Result<T,:dyn Error>` (`E` implementerar `Error`) | Vidgar en konkret feltyp till trait-objektet |
| `describe-error` | `(describe-error e)` | `E→string` (`E` implementerar `Error`) | Meddelandet och kedjan av orsaker man hittar genom att följa `source`, en orsak per rad. CL har ingen motsvarighet (Rusts "caused by") |

Om du implementerar `Error` för din egen feltyp kan den hanteras **på samma sätt** som de inbyggda
felen:

```lisp
(defstruct io-err (path string))
(impl Error io-err
  (message ((self Self)) string (append "io failed: " self::path))
  (source ((self Self)) Option<:dyn Error> (option::none)))

(defun open-it ((p string)) Result<i32, io-err>          ; den konkreta typen går in i E som den är
  (if (equal p "") (result::err (io-err::new "<empty>")) (result::ok 3)))

(defun describe ((e :dyn Error)) string (message e))     ; hantera vilket slag som helst enhetligt
(describe (io-err::new "/etc/app.conf"))
(describe (ParseIntError::ParseIntError "boom"))
```

För att samla flera feltyper i ett `Result` används `Result<T, :dyn Error>` (motsvarar Rusts
`Box<dyn Error>`), och konkreta fel vidgas med `as-dyn-error`. Eftersom det inte finns något `?` skrivs
den här konverteringen explicit:

```lisp
(defun run ((s string)) Result<i32, :dyn Error>
  (match (as-dyn-error (parse-int s))            ; ParseIntError -> :dyn Error
    ((ok n) (as-dyn-error (open-it (if (= n 0) "" "f"))))   ; io-err -> :dyn Error
    ((err e) (result::err e))))
```

**Typer och traits delar en namnrymd** (som i Rust). Inom en modul kan en `defstruct`/`defenum` och ett
trait inte ha samma namn, och att skriva ett traitnamn på en typplats rapporteras som
"`error` is a trait, not a type — write `:dyn error`".

Icke återhämtningsbara misslyckanden uttrycks med `panic`. För `panic` och `catch`/`throw`, se
[Syntaxreferensen](../syntax.md#8-icke-lokala-utgångar-catch--throw--unwind-protect); för
felhanteringspolicyn, se [kapitel 9 i samma](../syntax.md#9-policy-för-felhantering).
