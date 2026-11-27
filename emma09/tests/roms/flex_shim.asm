* The FLEX entry points W. Schwotzer's 6809 test suite (cputest.asm) calls,
* rebuilt on one output register for emma6k's test harness
* (tests/schwotzer_6809.rs). Written for emma6k; MIT, like the rest of it.
*
* The harness maps a write-only capture device at OUTPUT and stops when PC
* reaches WARMS, where the suite jumps when it's done.

OUTPUT  EQU   $C000
EOT     EQU   $04

        ORG   $CD00
COLDS   BRA   COLDS           ; $CD00, unused
WARMS   BRA   WARMS           ; $CD03: the suite's exit; the harness stops here
        ZMB   $CD18-*        ; pad to $CD18
PUTCHR  JMP   PUTC            ; $CD18: print A
        ZMB   $CD1E-*        ; pad to $CD1E
PSTRNG  JMP   PSTR            ; $CD1E: CR/LF, then the EOT-terminated string at X
        ZMB   $CD24-*        ; pad to $CD24
PCRLF   JMP   CRLF            ; $CD24: print CR/LF
        ZMB   $CD39-*        ; pad to $CD39
OUTDEC  RTS                   ; $CD39, unused by the suite

        ZMB   $CD80-*        ; pad to $CD80
PUTC    STA   OUTPUT
        RTS
CRLF    PSHS  A
        LDA   #$0D
        STA   OUTPUT
        LDA   #$0A
        STA   OUTPUT
        PULS  A,PC
PSTR    PSHS  A
        BSR   CRLF
PSTR1   LDA   ,X+
        CMPA  #EOT
        BEQ   PSTR2
        STA   OUTPUT
        BRA   PSTR1
PSTR2   PULS  A,PC
