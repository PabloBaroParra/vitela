# R8 rules for the release build. Everything not listed here is shrunk and
# obfuscated; these are the classes reached only through reflection or JNI.

# JNA binds native symbols by reflection: it reads the Library interface's
# method names, Structure field names (@FieldOrder) and Callback signatures at
# run time, and libjnidispatch calls back into com.sun.jna by name.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keepclassmembers class * extends com.sun.jna.** { *; }
# Desktop-only JNA code paths that never run on Android.
-dontwarn java.awt.**
-dontwarn com.sun.jna.**

# UniFFI's generated bindings: the UniffiLib interface method names must match
# the exported uniffi_* symbols in libpdf_ffi.so, and its RustBuffer /
# ForeignFuture structures are laid out by field name. Renaming any of it
# breaks the native boundary at run time, not at build time.
-keep class dev.vitela.pdf.uniffi.** { *; }

# Found through META-INF/services by PdfCoreProvider (ServiceLoader).
-keep class dev.vitela.pdf.generated.GeneratedPdfCoreFactory { <init>(); }
-keep interface dev.vitela.pdf.core.PdfCoreFactory
