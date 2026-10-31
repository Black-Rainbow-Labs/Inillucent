# The JNI glue finds Native.failure by name, and calls into Native's external functions by name, so
# an application that shrinks its code must keep them.
-keep class com.blackrainbowlabs.inillucent.Native { *; }
-keep class com.blackrainbowlabs.inillucent.InillucentException { *; }
-keep class com.blackrainbowlabs.inillucent.UnsupportedException { *; }
