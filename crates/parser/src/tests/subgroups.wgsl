enable subgroups;

@compute @workgroup_size(64)
fn main() {
    test_subgroup();
    test_quad();
}

fn test_subgroup() {
    var val_f32 = 1.0;
    var val_u32 = 1u;
    var val_i32 = 1;
    var val_bool = true;

    // --- 17.12.1. subgroupAdd ---
    let r_add = subgroupAdd(val_f32);

    // --- 17.12.1.1. subgroupExclusiveAdd ---
    let r_ex_add = subgroupExclusiveAdd(val_f32);

    // --- 17.12.1.2. subgroupInclusiveAdd ---
    let r_in_add = subgroupInclusiveAdd(val_f32);

    // --- 17.12.2. subgroupAll ---
    let r_all = subgroupAll(val_bool);

    // --- 17.12.3. subgroupAnd ---
    let r_and = subgroupAnd(val_u32);

    // --- 17.12.4. subgroupAny ---
    let r_any = subgroupAny(val_bool);

    // --- 17.12.5. subgroupBallot ---
    let r_ballot = subgroupBallot(val_bool);

    // --- 17.12.6. subgroupBroadcast ---
    let r_bcast = subgroupBroadcast(val_f32, 0u);

    // --- 17.12.6.1. subgroupBroadcastFirst ---
    let r_bcast_first = subgroupBroadcastFirst(val_f32);

    // --- 17.12.7. subgroupElect ---
    let r_elect = subgroupElect();

    // --- 17.12.8. subgroupMax ---
    let r_max = subgroupMax(val_f32);

    // --- 17.12.9. subgroupMin ---
    let r_min = subgroupMin(val_f32);

    // --- 17.12.10. subgroupMul ---
    let r_mul = subgroupMul(val_f32);

    // --- 17.12.10.1. subgroupExclusiveMul ---
    let r_ex_mul = subgroupExclusiveMul(val_f32);

    // --- 17.12.10.2. subgroupInclusiveMul ---
    let r_in_mul = subgroupInclusiveMul(val_f32);

    // --- 17.12.11. subgroupOr ---
    let r_or = subgroupOr(val_u32);

    // --- 17.12.12. subgroupShuffle ---
    let r_shuffle = subgroupShuffle(val_f32, 1u);

    // --- 17.12.12.1. subgroupShuffleDown ---
    let r_shuffle_down = subgroupShuffleDown(val_f32, 1u);

    // --- 17.12.12.2. subgroupShuffleUp ---
    let r_shuffle_up = subgroupShuffleUp(val_f32, 1u);

    // --- 17.12.12.3. subgroupShuffleXor ---
    let r_shuffle_xor = subgroupShuffleXor(val_f32, 1u);

    // --- 17.12.13. subgroupXor ---
    let r_xor = subgroupXor(val_u32);
}

fn test_quad() {
    var val_f32 = 1.0;

    // --- 17.13.1. quadBroadcast ---
    let r_quad_bcast = quadBroadcast(val_f32, 0u);

    // --- 17.13.2. quadSwapDiagonal ---
    let r_quad_swap_diag = quadSwapDiagonal(val_f32);

    // --- 17.13.3. quadSwapX ---
    let r_quad_swap_x = quadSwapX(val_f32);

    // --- 17.13.4. quadSwapY ---
    let r_quad_swap_y = quadSwapY(val_f32);
}