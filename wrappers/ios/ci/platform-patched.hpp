/*
Workaround to disable clock_gettime since it is only available on iOS 10+.
Taken from https://github.com/evernym/libzmq-ios
*/

/* src/platform.hpp.  Generated from platform.hpp.in by configure.  */
/* src/platform.hpp.in.  Generated from configure.ac by autoheader.  */

/* Define to 1 if you have the <alloca.h> header file. */
#define HAVE_ALLOCA_H 1

/* Define to 1 if you have the <arpa/inet.h> header file. */
#define HAVE_ARPA_INET_H 1

/* Define to 1 if you have the `clock_gettime' function. */
#undef HAVE_CLOCK_GETTIME

/* define if the compiler supports basic C++11 syntax */
#define HAVE_CXX11 1

/* Define to 1 if you have the declaration of `LOCAL_PEERCRED', and to 0 if
   you don't. */
#define HAVE_DECL_LOCAL_PEERCRED 0

/* Define to 1 if you have the declaration of `SO_PEERCRED', and to 0 if you
   don't. */
#define HAVE_DECL_SO_PEERCRED 0

/* Define to 1 if you have the <dlfcn.h> header file. */
#define HAVE_DLFCN_H 1

/* Define to 1 if you have the <errno.h> header file. */
#define HAVE_ERRNO_H 1

/* Define to 1 if you have the `fork' function. */
#define HAVE_FORK 1

/* Define to 1 if you have the `freeifaddrs' function. */
#define HAVE_FREEIFADDRS 1

/* Define to 1 if you have the `gethrtime' function. */
/* #undef HAVE_GETHRTIME */

/* Define to 1 if you have the `getifaddrs' function. */
#define HAVE_GETIFADDRS 1

/* Define to 1 if you have the `gettimeofday' function. */
#define HAVE_GETTIMEOFDAY 1

/* Define to 1 if you have the <gssapi/gssapi_generic.h> header file. */
/* #undef HAVE_GSSAPI_GSSAPI_GENERIC_H */

/* Define to 1 if you have the <ifaddrs.h> header file. */
#define HAVE_IFADDRS_H 1

/* Define to 1 if you have the <inttypes.h> header file. */
#define HAVE_INTTYPES_H 1

/* Define to 1 if you have the `iphlpapi' library (-liphlpapi). */
/* #undef HAVE_LIBIPHLPAPI */

/* Define to 1 if you have the `nsl' library (-lnsl). */
/* #undef HAVE_LIBNSL */

/* Define to 1 if you have the `pthread' library (-lpthread). */
#define HAVE_LIBPTHREAD 1

/* Define to 1 if you have the `rpcrt4' library (-lrpcrt4). */
/* #undef HAVE_LIBRPCRT4 */

/* Define to 1 if you have the `rt' library (-lrt). */
/* #undef HAVE_LIBRT */

/* Define to 1 if you have the `socket' library (-lsocket). */
/* #undef HAVE_LIBSOCKET */

/* The libunwind library is to be used */
/* #undef HAVE_LIBUNWIND */

/* Define to 1 if you have the `ws2_32' library (-lws2_32). */
/* #undef HAVE_LIBWS2_32 */

/* Define to 1 if you have the <limits.h> header file. */
#define HAVE_LIMITS_H 1

/* Define to 1 if you have the <memory.h> header file. */
#define HAVE_MEMORY_H 1

/* Define to 1 if you have the `memset' function. */
#define HAVE_MEMSET 1

/* Define to 1 if you have the `mkdtemp' function. */
#define HAVE_MKDTEMP 1

/* Define to 1 if you have the <netinet/in.h> header file. */
#define HAVE_NETINET_IN_H 1

/* Define to 1 if you have the <netinet/tcp.h> header file. */
#define HAVE_NETINET_TCP_H 1

/* Define to 1 if you have the `perror' function. */
#define HAVE_PERROR 1

/* Define to 1 if you have the `posix_memalign' function. */
#define HAVE_POSIX_MEMALIGN 1

/* Define to 1 if you have the `socket' function. */
#define HAVE_SOCKET 1

/* Define to 1 if stdbool.h conforms to C99. */
#define HAVE_STDBOOL_H 1

/* Define to 1 if you have the <stddef.h> header file. */
#define HAVE_STDDEF_H 1

/* Define to 1 if you have the <stdint.h> header file. */
#define HAVE_STDINT_H 1

/* Define to 1 if you have the <stdlib.h> header file. */
#define HAVE_STDLIB_H 1

/* Define to 1 if you have the <strings.h> header file. */
#define HAVE_STRINGS_H 1

/* Define to 1 if you have the <string.h> header file. */
#define HAVE_STRING_H 1

/* Define to 1 if you have the <sys/eventfd.h> header file. */
/* #undef HAVE_SYS_EVENTFD_H */

/* Define to 1 if you have the <sys/socket.h> header file. */
#define HAVE_SYS_SOCKET_H 1

/* Define to 1 if you have the <sys/stat.h> header file. */
#define HAVE_SYS_STAT_H 1

/* Define to 1 if you have the <sys/time.h> header file. */
#define HAVE_SYS_TIME_H 1

/* Define to 1 if you have the <sys/types.h> header file. */
#define HAVE_SYS_TYPES_H 1

/* Define to 1 if you have the <sys/uio.h> header file. */
#define HAVE_SYS_UIO_H 1

/* Define to 1 if you have the <time.h> header file. */
#define HAVE_TIME_H 1

/* Define to 1 if you have the <unistd.h> header file. */
#define HAVE_UNISTD_H 1

/* Define to 1 if you have the <windows.h> header file. */
/* #undef HAVE_WINDOWS_H */

/* Define to 1 if the system has the type `_Bool'. */
/* #undef HAVE__BOOL */

/* Define to the sub-directory in which libtool stores uninstalled libraries.
   */
#define LT_OBJDIR ".libs/"

/* Name of package */
#define PACKAGE "zeromq"

/* Define to the address where bug reports for this package should be sent. */
#define PACKAGE_BUGREPORT "zeromq-dev@lists.zeromq.org"

/* Define to the full name of this package. */
#define PACKAGE_NAME "zeromq"

/* Define to the full name and version of this package. */
#define PACKAGE_STRING "zeromq 4.2.0"

/* Define to the one symbol short name of this package. */
#define PACKAGE_TARNAME "zeromq"

/* Define to the home page for this package. */
#define PACKAGE_URL ""

/* Define to the version of this package. */
#define PACKAGE_VERSION "4.2.0"

/* Define as the return type of signal handlers (`int' or `void'). */
#define RETSIGTYPE void

/* Define to 1 if you have the ANSI C header files. */
#define STDC_HEADERS 1

/* Define to 1 if you can safely include both <sys/time.h> and <time.h>. */
#define TIME_WITH_SYS_TIME 1

/* Version number of package */
#define VERSION "4.2.0"

/* Enable militant API assertions */
/* #undef ZMQ_ACT_MILITANT */

/* Provide draft classes and methods */
#define ZMQ_BUILD_DRAFT_API 1

/* Force to use mutexes */
/* #undef ZMQ_FORCE_MUTEXES */

/* Have AIX OS */
/* #undef ZMQ_HAVE_AIX */

/* Have Android OS */
/* #undef ZMQ_HAVE_ANDROID */

/* Whether compiler has __atomic_Xxx intrinsics. */
#define ZMQ_HAVE_ATOMIC_INTRINSICS 1

/* Using curve encryption */
#define ZMQ_HAVE_CURVE 1

/* Have Cygwin */
/* #undef ZMQ_HAVE_CYGWIN */

/* Have DragonFly OS */
/* #undef ZMQ_HAVE_DRAGONFLY */

/* Have eventfd extension */
/* #undef ZMQ_HAVE_EVENTFD */

/* Have FreeBSD OS */
/* #undef ZMQ_HAVE_FREEBSD */

/* Have GNU/Hurd OS */
/* #undef ZMQ_HAVE_GNU */

/* Have HPUX OS */
/* #undef ZMQ_HAVE_HPUX */

/* Have ifaddrs.h header. */
#define ZMQ_HAVE_IFADDRS 1

/* Have Linux OS */
/* #undef ZMQ_HAVE_LINUX */

/* Have LOCAL_PEERCRED socket option */
/* #undef ZMQ_HAVE_LOCAL_PEERCRED */

/* Have MinGW */
/* #undef ZMQ_HAVE_MINGW */

/* Have NetBSD OS */
/* #undef ZMQ_HAVE_NETBSD */

/* Have NORM protocol extension */
/* #undef ZMQ_HAVE_NORM */

/* Have OpenBSD OS */
/* #undef ZMQ_HAVE_OPENBSD */

/* Have OpenPGM extension */
/* #undef ZMQ_HAVE_OPENPGM */

/* Have DarwinOSX OS */
#define ZMQ_HAVE_OSX 1

/* Have QNX Neutrino OS */
/* #undef ZMQ_HAVE_QNXNTO */

/* Whether SOCK_CLOEXEC is defined and functioning. */
/* #undef ZMQ_HAVE_SOCK_CLOEXEC */

/* Have Solaris OS */
/* #undef ZMQ_HAVE_SOLARIS */

/* Whether SO_KEEPALIVE is supported. */
/* #undef ZMQ_HAVE_SO_KEEPALIVE */

/* Have SO_PEERCRED socket option */
/* #undef ZMQ_HAVE_SO_PEERCRED */

/* Whether TCP_KEEPALIVE is supported. */
/* #undef ZMQ_HAVE_TCP_KEEPALIVE */

/* Whether TCP_KEEPCNT is supported. */
/* #undef ZMQ_HAVE_TCP_KEEPCNT */

/* Whether TCP_KEEPIDLE is supported. */
/* #undef ZMQ_HAVE_TCP_KEEPIDLE */

/* Whether TCP_KEEPINTVL is supported. */
/* #undef ZMQ_HAVE_TCP_KEEPINTVL */

/* Have TIPC support */
/* #undef ZMQ_HAVE_TIPC */

/* Have uio.h header. */
#define ZMQ_HAVE_UIO 1

/* Have VMCI transport */
/* #undef ZMQ_HAVE_VMCI */

/* Have Windows OS */
/* #undef ZMQ_HAVE_WINDOWS */

/* Use 'devpoll' polling system */
/* #undef ZMQ_USE_DEVPOLL */

/* Use 'epoll' polling system */
/* #undef ZMQ_USE_EPOLL */

/* Use 'kqueue' polling system */
#define ZMQ_USE_KQUEUE 1

/* Using libsodium for curve encryption */
/* #undef ZMQ_USE_LIBSODIUM */

/* Use 'poll' polling system */
/* #undef ZMQ_USE_POLL */

/* Use 'pollset' polling system */
/* #undef ZMQ_USE_POLLSET */

/* Use 'select' polling system */
/* #undef ZMQ_USE_SELECT */

/* Using tweetnacl for curve encryption */
#define ZMQ_USE_TWEETNACL 1

/* Define for Solaris 2.5.1 so the uint32_t typedef from <sys/synch.h>,
   <pthread.h>, or <semaphore.h> is not used. If the typedef were allowed, the
   #define below would cause a syntax error. */
/* #undef _UINT32_T */

/* Define to empty if `const' does not conform to ANSI C. */
/* #undef const */

/* Define to `__inline__' or `__inline' if that's what the C compiler
   calls it, or to nothing if 'inline' is not supported under any name.  */
#ifndef __cplusplus
/* #undef inline */
#endif

/* Define to `unsigned int' if <sys/types.h> does not define. */
/* #undef size_t */

/* Define to `int' if <sys/types.h> does not define. */
/* #undef ssize_t */

/* Define to the type of an unsigned integer type of width exactly 32 bits if
   such a type exists and the standard includes do not define it. */
/* #undef uint32_t */

/* Define to empty if the keyword `volatile' does not work. Warning: valid
   code using `volatile' can become incorrect without. Disable with care. */
/* #undef volatile */