fn main() {
    let mut build = cc::Build::new();

    build
        .cpp(false)
        .define("NDEBUG", "1")
        .define("LSM_MUTEX_PTHREADS", "1")
        .file("sqlite/ext/lsm1/lsm_ckpt.c")
        .file("sqlite/ext/lsm1/lsm_file.c")
        .file("sqlite/ext/lsm1/lsm_log.c")
        .file("sqlite/ext/lsm1/lsm_main.c")
        .file("sqlite/ext/lsm1/lsm_mem.c")
        .file("sqlite/ext/lsm1/lsm_mutex.c")
        .file("sqlite/ext/lsm1/lsm_shared.c")
        .file("sqlite/ext/lsm1/lsm_sorted.c")
        .file("sqlite/ext/lsm1/lsm_str.c")
        .file("sqlite/ext/lsm1/lsm_tree.c")
        .file("sqlite/ext/lsm1/lsm_unix.c")
        .file("sqlite/ext/lsm1/lsm_win32.c")
        .file("sqlite/ext/lsm1/lsm_varint.c")
        .include("sqlite/ext/lsm1/lsm_varint")
        .warnings(false);

    build.compile("lsm_extension");
}
