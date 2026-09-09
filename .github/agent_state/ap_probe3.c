// C107 probe: (1) 4-way parallel pinned spins with per-CPU timing,
// (2) shared-atomic race: 4 pinned kids fetch_add a MAP_SHARED counter
// 200000x each; parent prints final (expect 800000, lost updates if broken).
#define _GNU_SOURCE
#include <sched.h>
#include <stdio.h>
#include <stdlib.h>
#include <errno.h>
#include <unistd.h>
#include <time.h>
#include <sys/wait.h>
#include <sys/mman.h>
static void pin(int cpu) {
    cpu_set_t m; CPU_ZERO(&m); CPU_SET(cpu, &m);
    if (sched_setaffinity(0, sizeof(m), &m) != 0) {
        printf("CPU%d-AFFINITY-FAIL errno=%d\n", cpu, errno);
        _exit(2);
    }
}
static long ms_now(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec * 1000L + ts.tv_nsec / 1000000L;
}
int main(void) {
    setvbuf(stdout, NULL, _IONBF, 0);
    printf("PROBE3-START\n");
    for (int cpu = 0; cpu <= 3; cpu++) {
        if (fork() == 0) {
            pin(cpu);
            long t0 = ms_now();
            volatile unsigned long acc = 0;
            for (unsigned long i = 0; i < 4000000UL; i++) acc += i;
            printf("SPIN-CPU%d acc=%lu delta_ms=%ld\n", cpu, acc, ms_now() - t0);
            _exit(0);
        }
    }
    for (int i = 0; i < 4; i++) { int st = 0; wait(&st); printf("SPIN-REAPED status=%d\n", st); }
    long *ctr = mmap(NULL, 4096, PROT_READ|PROT_WRITE, MAP_SHARED|MAP_ANONYMOUS, -1, 0);
    if (ctr == MAP_FAILED) { printf("MMAP-FAIL errno=%d\n", errno); return 1; }
    *ctr = 0;
    for (int cpu = 0; cpu <= 3; cpu++) {
        if (fork() == 0) {
            pin(cpu);
            for (int i = 0; i < 200000; i++) __atomic_fetch_add(ctr, 1L, __ATOMIC_SEQ_CST);
            _exit(0);
        }
    }
    for (int i = 0; i < 4; i++) { int st = 0; wait(&st); }
    printf("ATOMIC-FINAL=%ld (expect 800000)\n", __atomic_load_n(ctr, __ATOMIC_SEQ_CST));
    printf("PROBE3-DONE\n");
    return 0;
}
