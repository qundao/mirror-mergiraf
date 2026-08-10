(define-module (test-packages)
  #:use-module (guix)
  #:use-module ((guix licenses) #:prefix license:)
  #:use-module (gnu packages)
  #:use-module (guix build-system gnu))

(define-public base-package-1
  (package
    (name "base-package-1")
    (version "1.2")
    (source
      (origin
        (method url-fetch)
        (uri "mirror://gnu/base-package/1.2.tgz")
        (sha256 (base32 "hash12"))))
    (build-system gnu-build-system)
    (arguments (list #:configure-flags #~(list "--enable-frobnicate")))
    (home-page "https://example.com/base-package")
    (synopsis "Base package")
    (description "Base package one")
    (license license:gpl3+)))

(define-public base-package-2
  (package
    (name "base-package-2")
    (version "2.0")
    (source
      (origin
        (method url-fetch)
        (uri "mirror://gnu/base-package/2.0.tgz")
        (sha256 (base32 "hash20"))))
    (build-system gnu-build-system)
    (arguments (list #:configure-flags #~(list "--enable-frobnicate-advanced")))
    (home-page "https://example.com/base-package-v2")
    (synopsis "Base package")
    (description "Base package two")
    (license license:gpl3+)))
