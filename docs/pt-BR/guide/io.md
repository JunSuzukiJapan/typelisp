<!-- translated-from: docs/ja/guide/io.md @ 6a9ad7ad4f7c098262628978d36af26e7cdd0401 -->
# E/S de arquivos, streams e rede

Este guia mostra o básico de ler e escrever arquivos, nomes de caminho e comunicação por sockets. As listas
de funções estão em [Streams e arquivos](../reference/functions/streams-files.md) e
[Rede](../reference/functions/network.md).

## 1. As falhas voltam como `Result`

Operações que podem falhar conforme o ambiente, como abrir um arquivo ou conectar, devolvem um `Result`. Um
arquivo inexistente não é um erro do programa, então não causa `panic`. Separe os resultados com `match`.

```lisp
(match (read-file-string "config.txt")
  ((ok text) (println "~a" text))
  ((err e) (println "error: ~a" (message e))))
;; se o arquivo não existir:
;; error: config.txt: No such file or directory (os error 2)
```

Quando você sabe que uma operação não pode falhar, ou em um script pequeno em que parar na falha não tem
problema, `unwrap` tira o valor. Se for um `Err`, ocorre um panic.

## 2. Ler e escrever um arquivo inteiro

As funções mais simples tratam o arquivo inteiro de uma vez.

```lisp
(unwrap (write-file-string "copy.txt" "abc\n"))
(read-file-string "copy.txt")      ; => (ok "abc\n")
(read-file-lines "copy.txt")       ; => (ok #("abc"))
```

## 3. Ler e escrever com streams

Para ler ou escrever aos poucos, abra um stream com `with-open-file`. Qualquer que seja a forma de sair do
corpo, o stream é fechado. O valor é `Result<valor do corpo, FileError>`.

```lisp
;; escrever
(match (with-open-file (out "notes.txt" direction-output)
         (write-line out "first")
         (write-line out "second")
         (format out "total: ~a~%" 2))
  ((ok _) ())
  ((err e) (println "write failed: ~a" (message e))))

;; ler uma linha por vez
(match (with-open-file (in "notes.txt" direction-input)
         (let ((n 0))
           (loop (match (read-line in)
                   ((some line) (progn (setf n (+ n 1)) (println "~a: ~a" n line)))
                   ((none) (break))))
           n))
  ((ok n) (println "~a lines" n))
  ((err e) (println "read failed: ~a" (message e))))
```

- Há três direções para abrir um arquivo: `direction-input` (leitura), `direction-output` (escrita; o conteúdo
  existente é descartado) e `direction-append` (acrescentar no final).
- `read-line` devolve `none` no fim do arquivo.
- Se você abrir um arquivo com `open-file` em vez de `with-open-file`, sempre chame `close`. O GC não fecha
  streams.

Para ler e escrever bytes, abra o arquivo com `open-binary-input` / `open-binary-output` e use `read-byte` /
`write-byte`. Streams de caracteres e streams de bytes são tipos diferentes, então tentar ler bytes de um
stream de caracteres é um erro de tipo.

### Usar uma string como stream

```lisp
(with-output-to-string (s)
  (write-string s "a")
  (write-string s "b"))                  ; => "ab"

(with-input-from-string (s "line1\nline2")
  (read-line s))                         ; => (some "line1")
```

### Entrada e saída padrão

`*standard-input*`, `*standard-output*` e `*error-output*` também são streams.
`(read-line *standard-input*)` lê uma linha.

### Tornar genéricas as funções de leitura e escrita

Cada tipo de stream é um tipo próprio, mas as operações em comum estão reunidas em traits. Uma função que
recebe seu argumento com `(where (CharInput S))` pode ler de um arquivo, de uma string ou de um socket.

```lisp
(defun count-lines<S> ((s S)) int (where (CharInput S))
  (let ((n 0))
    (loop (match (read-line s)
            ((some _) (setf n (+ n 1)))
            ((none) (break))))
    n))
```

## 4. Nomes de caminho

As funções que recebem um nome de arquivo aceitam tanto uma string quanto um `pathname`. Use um `pathname`
para dividir um caminho em partes ou para construí-lo.

```lisp
(let ((p (parse-namestring "/var/log/app.tar.gz")))
  (pathname-directory p)   ; => #("var" "log")
  (pathname-name p)        ; => (some "app.tar")
  (pathname-type p))       ; => (some "gz")

;; trocar só a extensão
(namestring (merge-pathnames (make-pathname :type "json") (to-pathname "data/in.csv")))
;; => "data/in.json"

;; pôr um nome de arquivo dentro de um diretório
(namestring (merge-pathnames (make-pathname :name "today" :type "log") "/var/log/"))
;; => "/var/log/today.log"
```

Operações com o sistema de arquivos:

```lisp
(unwrap (ensure-directories-exist "out/deep/"))   ; cria-o junto com os pais
(probe-file "out/deep")                           ; => true (existe)
(directory-p "out/deep")                          ; => true
(directory "out")                                 ; => a lista do conteúdo
(delete-file "copy.txt")
(rename-file "a.txt" "b.txt")
```

O separador é sempre `/`. Não há componentes de host, dispositivo nem versão como nos nomes de caminho do
Common Lisp, nem curingas nem nomes de caminho lógicos.

## 5. TCP

Uma conexão de socket também é um stream, então `read-line` e `write-line` funcionam sobre ela como estão.

### Servidor

O padrão básico é iniciar uma tarefa por conexão. `accept` e `read-line` param **só aquela tarefa** até os
dados chegarem, então o atendimento das outras conexões continua.

```lisp
(defun serve ((c socket-stream)) ()
  (loop (match (read-line c)
          ((some line) (write-line c (upcase line)))
          ((none) (break))))                  ; o outro lado fechou
  (close c))

(let ((l (unwrap (tcp-listen "127.0.0.1" 7777))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))
          ((err e) (println "accept: ~a" (message e))))))
```

`"127.0.0.1"` aceita conexões só desta máquina, e `"0.0.0.0"` as aceita em todas as interfaces. Sobre as
tarefas, consulte o [capítulo 12 da Referência de sintaxe](../reference/syntax.md#12-concorrência-tarefas).

### Cliente

`with-connection` conecta, executa o corpo e fecha a conexão no final. O valor é
`Result<valor do corpo, NetError>`.

```lisp
(match (with-connection (c "localhost" 7777)
         (write-line c "hello")
         (unwrap (read-line c)))
  ((ok reply) (println "reply: ~a" reply))      ; reply: HELLO
  ((err e) (println "~a" (message e))))
```

Para pôr um limite de tempo em uma operação, passe `:timeout` (em segundos).

```lisp
(tcp-connect "example.com" 80 :timeout 5.0)
(accept l :timeout 30.0)
(if (wait-readable c 5.0) (read-line c) (option::none))   ; pôr um relógio na próxima leitura
```

### Quando o outro lado desconecta

Se o outro lado reiniciar a conexão ou cortá-la no meio, não ocorre `panic`. As leituras seguintes devolvem
`none`, e as escritas são descartadas silenciosamente. Para saber se a conexão foi fechada de forma limpa ou
falhou, verifique `(socket-error c)`.

## 6. Resolução de nomes (DNS)

Não há uma função dedicada à resolução de nomes. Passar um nome de host para `tcp-connect`, `tls-connect` ou
`send-to` o resolve dentro delas. A resolução acontece em outra thread, então as demais tarefas continuam em
execução enquanto ela espera. Se um nome tiver vários endereços, eles são tentados em sequência.

Se o nome não puder ser resolvido, um `Err` é devolvido.

```lisp
(match (tcp-connect "no-such-host.invalid" 80 :timeout 3.0)
  ((ok c) (close c))
  ((err e) (println "~a" (message e))))
;; resolve: no-such-host.invalid:80: failed to lookup address information: ...
```

## 7. TLS

`tls-connect` é usado como `tcp-connect` e realiza o handshake TLS depois de conectar. O certificado do
servidor é verificado contra os certificados raiz padrão. O resultado é um `socket-stream` comum, então ler e
escrever funcionam como com TCP.

```lisp
(match (tls-connect "example.com" 443 :timeout 10.0)
  ((ok c)
   (progn
     (write-string c "GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
     (println "~a" (unwrap (read-line c)))     ; HTTP/1.1 200 OK
     (close c)))
  ((err e) (println "tls failed: ~a" (message e))))
```

Um servidor TLS é criado passando para `tls-listen` os arquivos de certificado e de chave privada (PEM).

```lisp
(let ((l (unwrap (tls-listen "0.0.0.0" 8443 "cert.pem" "key.pem"))))
  (loop (match (accept l)
          ((ok c) (progn (task (serve c)) ()))   ; o serve do TCP funciona como está
          ((err e) (println "accept: ~a" (message e))))))
```

Ao testar com um certificado autoassinado, passe `:ca-file "cert.pem"` no lado do cliente para que ele confie
nesse certificado. O TLS mútuo e servir vários sites a partir de um mesmo listener são tratados em
[Rede](../reference/functions/network.md#2-tcp--tls--domínio-unix).

## 8. UDP

UDP não é um stream; ele envia e recebe um datagrama por vez. Os dados são uma sequência de bytes
(`Vector<int>`); converta de e para strings com `string->utf8` / `utf8->string`.

```lisp
(let ((a (unwrap (udp-bind "127.0.0.1" 0)))          ; até para enviar é preciso um socket; 0 deixa a porta para o SO
      (b (unwrap (udp-bind "127.0.0.1" 9999))))
  (unwrap (send-to a "localhost" 9999 (string->utf8 "ping")))
  (match (recv-from b :timeout 1.0)
    ((ok d) (println "~a from ~a" (unwrap (utf8->string (bytes d))) (from d)))
    ((err e) (println "~a" (message e)))))
;; ping from 127.0.0.1:59161
```

`(from d)` é o `ip:porta` do remetente, que pode ser usado como está como destino de `send-to`.

## 9. Sockets de domínio Unix

Passe o caminho do arquivo de socket para `unix-listen` / `unix-connect`. O valor obtido é o mesmo
`socket-stream` do TCP. `unix-listen` devolve um `Err` se o arquivo já existir. Fechar o listener remove o
arquivo.
