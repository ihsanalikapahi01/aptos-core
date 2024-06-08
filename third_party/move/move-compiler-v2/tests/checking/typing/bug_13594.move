module 0x42::test {

    struct R has store, key, drop {
        dummy_field: bool,
    }

    public entry fun test(addr: address) acquires R {
        let R {  } = move_from<R>(addr);
    }

    public entry fun test2(s: &signer) {
        let r = R {};
        move_to<R>(s, r);
    }

    struct T has store, key, drop {
    }

    public entry fun test3(addr: address) acquires T {
        let T {  } = move_from<T>(addr);
    }

    public entry fun test4(s: &signer) {
        let r = T {};
        move_to<T>(s, r);
    }

    struct G has store, key, drop {
        dummy_field_1: bool,
    }

    public entry fun test5(addr: address) acquires G {
        let G {  } = move_from<G>(addr);
    }

    public entry fun test6(s: &signer) {
        let r = G {};
        move_to<G>(s, r);
    }

}
