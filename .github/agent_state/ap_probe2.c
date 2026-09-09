// C106 probe: per-CPU fork + sched_yield + nanosleep (200ms) with
// clock_gettime deltas. Sequential per CPU for deterministic output.
// Missing line / hang => that CPU doesn't schedule; sleep delta far
// above 200ms on an AP => its timer tick isn't firing.
#define _GNU_SOURCE
#include <sched.h>
#include <stdio.h>
#include <stdlib.h>
#include <errno.h>
#include <unistd.h>
#include <time.h>
#include <sys/wait.h>
static void pin(int cpu) {
    cpu_set_t m; CPU_ZERO(&m); CPU_SET(cpu, &m);
    if (sched_setaffinity(0, sizeof(m), &m) != 0) {
        printf("CPU%d-AFFINITY-FAIL errno=%d\n", cpu, errno);
        _exit(2);
    }
}
static long ms_now(int *ok) {
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0) { *ok = 0; return 0; }
    *ok = 1;
    return ts.tv_sec * 1000L + ts.tv_nsec / 1000000L;
}
int main(void) {
    setvbuf(stdout, NULL, _IONBF, 0);
    printf("PROBE2-START\n");
    for (int cpu = 0; cpu <= 3; cpu++) {
        pid_t p = fork();
        if (p == 0) { pin(cpu); printf("FORK-CPU%d-OK\n", cpu); _exit(0); }
        int st = 0; waitpid(p, &st, 0); printf("FORK-CPU%d-REAPED status=%d\n", cpu, st);
        p = fork();
        if (p == 0) {
            pin(cpu);
            for (volatile int i = 0; i < 20000; i++) sched_yield();
            printf("YIELD-CPU%d-OK\n", cpu); _exit(0);
        }
        st = 0; waitpid(p, &st, 0); printf("YIELD-CPU%d-REAPED status=%d\n", cpu, st);
        p = fork();
        if (p == 0) {
            pin(cpu);
            int ok1 = 0, ok2 = 0;
            long t0 = ms_now(&ok1);
            struct timespec rq = {0, 200*1000*1000L};
            nanosleep(&rq, NULL);
            long t1 = ms_now(&ok2);
            if (ok1 && ok2) printf("SLEEP-CPU%d delta_ms=%ld (expect~200)\n", cpu, t1 - t0);
            else printf("SLEEP-CPU%d-OK noclock\n", cpu);
            _exit(0);
        }
        st = 0; waitpid(p, &st, 0); printf("SLEEP-CPU%d-REAPED status=%d\n", cpu, st);
    }
    printf("PROBE2-DONE\n");
    return 0;
}
