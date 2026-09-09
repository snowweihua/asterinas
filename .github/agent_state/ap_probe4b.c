// Discriminator: vfork + clone(SIGCHLD) + signal-to-self, NO pthread/TLS.
#define _GNU_SOURCE
#include <sched.h>
#include <stdio.h>
#include <stdlib.h>
#include <errno.h>
#include <unistd.h>
#include <signal.h>
#include <sys/wait.h>
static volatile sig_atomic_t got_sig = 0;
static void handler(int s) { (void)s; got_sig = 1; }
static int child_fn(void *a) {
    printf("CLONE-CHILD arg=%d (expect 5)\n", *(int *)a);
    _exit(7);
    return 0;
}
int main(void) {
    setvbuf(stdout, NULL, _IONBF, 0);
    printf("PROBE4B-START\n");
    pid_t p = vfork();
    if (p == 0) _exit(11);
    if (p < 0) printf("VFORK-FAIL errno=%d\n", errno);
    else { int st = 0; waitpid(p, &st, 0); printf("VFORK status=%d (expect %d)\n", st, 11 << 8); }
    static char clstack[65536];
    static int carg = 5;
    pid_t c = clone(child_fn, clstack + sizeof(clstack), SIGCHLD, &carg);
    if (c < 0) printf("CLONE-FAIL errno=%d\n", errno);
    else { int st = 0; waitpid(c, &st, 0); printf("CLONE status=%d (expect %d)\n", st, 7 << 8); }
    struct sigaction sa;
    sa.sa_handler = handler; sigemptyset(&sa.sa_mask); sa.sa_flags = 0;
    if (sigaction(SIGUSR1, &sa, NULL) != 0) printf("SIGACTION-FAIL errno=%d\n", errno);
    else { kill(getpid(), SIGUSR1); printf("SIGNAL got=%d (expect 1)\n", got_sig); }
    printf("PROBE4B-DONE\n");
    return 0;
}
