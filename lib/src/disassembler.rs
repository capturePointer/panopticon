use value::Rvalue;
use mnemonic::Mnemonic;
use guard::Guard;
use std::rc::Rc;
use num::traits::*;
use std::fmt::{Display,Debug};
use std::slice::Iter;
use std::ops::{BitAnd,BitOr,Shl,Shr,Not};
use std::collections::HashMap;
use std::mem::size_of;
use codegen::CodeGen;
use layer::LayerIter;

pub trait Token: Clone + Zero + One + Debug + Not + BitOr + BitAnd + Shl<usize> + Shr<usize> + NumCast + PartialEq + Eq
where <Self as Not>::Output: NumCast,
      <Self as BitOr>::Output: NumCast,
      <Self as BitAnd>::Output: NumCast,
      <Self as Shl<usize>>::Output: NumCast,
      <Self as Shr<usize>>::Output: NumCast,
{}

impl Token for u8 {}

pub type Action<I/*: Token*/> = fn(&mut State<I>) -> bool;

#[derive(Debug,Clone)]
pub struct State<I: Clone> {
    // in
    pub address: u64,
    pub tokens: Vec<I>,
    pub groups: Vec<(String,I)>,

    // out
    pub mnemonics: Vec<Mnemonic>,
    pub jumps: Vec<(Rvalue,Guard)>,

    next_address: u64,
}

impl<I: Clone> State<I> {
    pub fn new(a: u64) -> State<I> {
        State{
            address: a,
            tokens: vec!(),
            groups: vec!(),
            mnemonics: Vec::new(),
            jumps: Vec::new(),
            next_address: a,
        }
    }

    pub fn mnemonic<F: Fn(&CodeGen) -> ()>(&mut self,len: usize, n: &str, fmt: &str, ops: Vec<Rvalue>, f: F) {
        self.mnemonic_dynargs(len,n,fmt,|cg: &CodeGen| -> Vec<Rvalue> {
            f(cg);
            ops.clone()
        });
    }

    pub fn mnemonic_dynargs<F>(&mut self,len: usize, n: &str, fmt: &str, f: F)
    where F: Fn(&CodeGen) -> Vec<Rvalue> {
        let cg = CodeGen::new();
        let ops = f(&cg);

        self.mnemonics.push(Mnemonic::new(
                self.next_address..(self.next_address + (len as u64)),
                n.to_string(),
                fmt.to_string(),
                ops.iter(),
                cg.instructions.iter()));
        self.next_address += len as u64;
    }

    pub fn jump(&mut self,v: Rvalue,g: Guard) {
        self.jumps.push((v,g));
    }
}

#[derive(Clone)]
pub struct Match<I: Token> {
    patterns: Vec<(I,I)>,
    actions: Vec<Rc<Action<I>>>,
    groups: Vec<(String,Vec<I>)>
}

pub enum Expr<I: Token> {
    Pattern(String),
    Terminal(I),
    Subdecoder(Rc<Disassembler<I>>)
}

pub trait ToExpr<I: Token> {
    fn to_expr(&self) -> Expr<I>;
}

impl<'a,I: Token> ToExpr<I> for &'a str {
    fn to_expr(&self) -> Expr<I> {
        Expr::Pattern(self.to_string())
    }
}

impl<'a,I: Token> ToExpr<I> for Rc<Disassembler<I>> {
    fn to_expr(&self) -> Expr<I> {
        Expr::Subdecoder(self.clone())
    }
}

impl<I: Token> ToExpr<I> for usize {
    fn to_expr(&self) -> Expr<I> {
        Expr::Terminal(I::from::<usize>(*self).unwrap().clone())
    }
}

impl<I: Token> Expr<I> {
    pub fn matches(&self) -> Vec<Match<I>>
    where <I as Not>::Output: NumCast,
          <I as BitOr>::Output: NumCast,
          <I as BitAnd>::Output: NumCast,
          <I as Shl<usize>>::Output: NumCast
    {
        let mut pats = Vec::<(I,I)>::new();
        let mut grps = HashMap::<String,Vec<I>>::new();

        match self {
            &Expr::Pattern(ref s) => {
                let mut groups = HashMap::<String,I>::new();
                let mut cur_group = "".to_string();
                let mut read_pat = false; // false while reading torwards @
                let mut bit: isize = (size_of::<I>() * 8) as isize;
                let mut invmask = I::zero();
                let mut pat = I::zero();

                for c in s.chars() {
                    match c {
                        '@' => {
                            if read_pat {
                                panic!("Pattern syntax error: read '@' w/o name in '{}'",s);
                                //return Vec::new();
                            } else {
                                read_pat = true;

                                if cur_group == "" {
                                    panic!("Pattern syntax error: anonymous groups not allowed in '{}'",s);
                                    //return Vec::new();
                                }

                                groups.insert(cur_group.clone(),I::zero());
                            }
                        },
                        ' ' => {
                            read_pat = false;
                            cur_group = "".to_string();
                        },
                        '.' => {
                            if read_pat && cur_group != "" {
                                *groups.get_mut(&cur_group).unwrap() = cast(groups.get(&cur_group).unwrap().clone() | cast(I::one() << ((bit - 1) as usize)).unwrap()).unwrap();
                            }

                            bit -= 1;
                        },
                        '0' | '1' => {
                            if bit - 1 > 0 {
                                invmask = cast(invmask | cast(I::one() << ((bit - 1) as usize)).unwrap()).unwrap();
                            } else {
                                invmask = cast(invmask | I::one()).unwrap();
                            }

                            if c == '1' {
                                pat = cast(pat | cast(I::one() << ((bit - 1) as usize)).unwrap()).unwrap();
                            }

                            if read_pat && cur_group != "" {
                                *groups.get_mut(&cur_group).unwrap() = cast(groups.get(&cur_group).unwrap().clone() | cast(I::one() << ((bit - 1) as usize)).unwrap()).unwrap();
                            }

                            bit -= 1;
                        },
                        'a'...'z' | 'A'...'Z' => {
                            if read_pat {
                                panic!("Pattern syntax error: undelimited capture group name in '{}'",s);
                                //return Vec::new();
                            } else {
                                cur_group.push(c);
                            }
                        },
                        _ => {
                            panic!("Pattern syntax error: invalid character '{}' in '{}'",c,s);
                            //return Vec::new();
                        }
                    }
                }

                if bit != 0 {
                    panic!("Pattern syntax error: invalid pattern length");
                }

                pats.push((pat.clone(),cast(invmask.clone()).unwrap()));

                for g in groups {
                    if grps.contains_key(&g.0) {
                        grps.get_mut(&g.0).unwrap().push(g.1)
                    } else if g.1 != I::zero() {
                        grps.insert(g.0,vec!(g.1));
                    }
                }
            },
            &Expr::Terminal(ref i) => pats.push((i.clone(),cast(!I::zero()).unwrap())),
            &Expr::Subdecoder(ref m) => return m.matches.clone(),
        }

        vec!(Match::<I>{
            patterns: pats,
            groups: grps.iter().map(|x| (x.0.clone(),x.1.clone())).collect(),
            actions: vec!()
        })
    }
}

pub struct Disassembler<I: Token> {
    pub matches: Vec<Match<I>>,
    default: Option<Action<I>>,
}

impl<I: Token> Disassembler<I> {
    pub fn new() -> Disassembler<I> {
        Disassembler::<I> {
            matches: Vec::new(),
            default: None,
        }
    }

    pub fn set_default(&mut self,f: Action<I>) {
        self.default = Some(f);
    }

    fn combine_expr(mut i: Iter<Expr<I>>, a: Action<I>) -> Vec<Match<I>>
    where <I as Not>::Output: NumCast,
          <I as BitOr>::Output: NumCast,
          <I as BitAnd>::Output: NumCast,
          <I as Shl<usize>>::Output: NumCast
    {
        match i.next() {
            Some(e) => {
                let rest = Self::combine_expr(i,a);
                let mut ret = Vec::new();


                for mut _match in (*e).matches() {
                    for pre in &rest {
                        for x in &pre.patterns {
                            _match.patterns.push(x.clone());
                        }

                        for x in &pre.actions {
                            _match.actions.push(x.clone());
                        }

                        for x in &pre.groups {
                            for y in _match.groups.iter_mut() {
                                if y.0 == x.0 {
                                    for p in &x.1 {
                                        y.1.push(p.clone());
                                    }
                                }
                            }
                        }
                    }

                    ret.push(Match::<I>{
                        patterns: _match.patterns,
                        actions:_match.actions,
                        groups: _match.groups
                    });
                }

                ret
            },
            None => vec!(Match::<I>{
                patterns: vec!(),
                actions: vec!(Rc::new(a)),
                groups: vec!(),
            })
        }
    }

    pub fn add_expr(&mut self, e: Vec<Expr<I>>, a: Action<I>)
    where <I as Not>::Output: NumCast,
          <I as BitAnd>::Output: NumCast,
          <I as BitOr>::Output: NumCast,
          <I as Shl<usize>>::Output: NumCast
    {
        for x in Self::combine_expr(e.iter(),a) {
            self.matches.push(x);
        }
    }

    pub fn next_match(&self,i: &mut LayerIter, _st: State<I>) -> Option<State<I>>
    where <I as Not>::Output: NumCast,
          <I as BitAnd>::Output: NumCast,
          <I as BitOr>::Output: NumCast,
          <I as Shl<usize>>::Output: NumCast,
          <I as Shr<usize>>::Output: NumCast,
          I: Eq + PartialEq + Display
    {
        let mut tokens = Vec::<I>::new();
        let mut j = i.clone();
        let min_len = |len: usize, ts: &mut Vec<I>, j: &mut LayerIter| -> bool {
            if ts.len() >= len {
                true
            } else {
                for _ in ts.len()..len {
                    let mut tmp: I = I::zero();

                    for x in (0..size_of::<I>()) {
                        if let Some(Some(b)) = j.next() {
                            if x != 0 {
                                tmp = cast::<<I as Shl<usize>>::Output,I>(tmp << 8).unwrap();
                            }
                            tmp = cast(tmp | cast(b).unwrap()).unwrap();
                        } else {
                            return false;
                        }
                    }
                    ts.push(tmp);
                }

                (ts.len() >= len)
            }
        };

        for opt in &self.matches {
            let pattern = &opt.patterns;
            let actions = &opt.actions;

            if !min_len(pattern.len(),&mut tokens,&mut j) {
                continue;
            }

            let is_match = pattern.iter().zip(tokens.iter()).all(|p| {
                let pat = (p.0).0.clone();
                let msk = (p.0).1.clone();
                let tok = p.1.clone();

                cast::<<I as BitAnd>::Output,I>(msk & tok).unwrap() == pat
            });

            if is_match {
                let mut grps = HashMap::<String,I>::new();
                let mut st = _st.clone();

                for cap in &opt.groups {
                    let masks = &cap.1;
                    let mut res = grps.get(&cap.0).unwrap_or(&I::zero()).clone();

                    for tok_msk in tokens.iter().zip(masks.iter()) {
                        if *tok_msk.1 != I::zero() {
                            for rbit in (0..(size_of::<I>() * 8)) {
                                let bit = (size_of::<I>() * 8) - rbit - 1;
                                let mask = if bit > 0 {
                                    cast::<<I as Shl<usize>>::Output,I>(I::one() << bit).unwrap()
                                } else {
                                    I::one()
                                };

                                if cast::<<I as BitAnd>::Output,I>(mask.clone() & tok_msk.1.clone()).unwrap() != I::zero() {
                                    res = cast::<<I as Shl<usize>>::Output,I>(res << 1).unwrap();

                                    if cast::<<I as BitAnd>::Output,I>(tok_msk.0.clone() & tok_msk.1.clone()).unwrap() != I::zero() {
                                        res = cast::<<I as BitOr>::Output,I>(res | I::one()).unwrap();
                                    }
                                }
                            }
                        }
                    }

                    grps.insert(cap.0.clone(),res);
                }

                st.tokens = tokens.iter().take(pattern.len()).cloned().collect();
                st.groups = grps.iter().map(|x| (x.0.clone(),x.1.clone())).collect::<Vec<_>>();

                if actions.iter().all(|x| x(&mut st)) {
                    return Some(st);
                }
            }
        }

        if self.default.is_some() && min_len(1,&mut tokens,&mut j) {
            let mut st = _st.clone();

            st.tokens = vec!(tokens.iter().next().unwrap().clone());

            if self.default.unwrap()(&mut st) {
                return Some(st);
            }
        }

        None
    }
}

macro_rules! new_disassembler {
    ($ty:ty => $( [ $( $t:expr ),+ ] = $f:expr),+) => {
        {
            let mut dis = Disassembler::<$ty>::new();

            $({
                let mut __x = Vec::new();
                $(
                    __x.push($t.to_expr());
                )+
                fn a(a: &mut State<$ty>) -> bool { ($f)(a) };
                let fuc: Action<$ty> = a;
                dis.add_expr(__x,fuc);
            })+

            Rc::<Disassembler<$ty>>::new(dis)
        }
    };
    ($ty:ty => $( [ $( $t:expr ),+ ] = $f:expr),+, _ = $def:expr) => {
        {
            let mut dis = Disassembler::<$ty>::new();

            $({
                let mut __x = Vec::new();
                $(
                    __x.push($t.to_expr());
                )+
                fn a(a: &mut State<$ty>) -> bool { ($f)(a) };
                let fuc: Action<$ty> = a;
                dis.add_expr(__x,fuc);
            })+

            fn __def(st: &mut State<u8>) -> bool { ($def)(st) };
            dis.set_default(__def);

            Rc::<Disassembler<$ty>>::new(dis)
        }
    };

}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;
    use layer::OpaqueLayer;
    use guard::Guard;
    use value::Rvalue;
    use mnemonic::Bound;

    #[test]
    fn decode_macro() {
        let lock_prfx = new_disassembler!(u8 =>
            [ 0x06 ] = |_| { true }
        );

        new_disassembler!(u8 =>
            [ 22 , 21, lock_prfx ] = |_| { true },
            [ "....11 d@00"         ] = |_| true,
            [ "....11 d@00", ".. d@0011. 0" ] = |_| true
        );
    }

    fn fixture() -> (Rc<Disassembler<u8>>,Rc<Disassembler<u8>>,Rc<Disassembler<u8>>,OpaqueLayer) {
        let sub = new_disassembler!(u8 =>
            [ 2 ] = |st: &mut State<u8>| {
                let next = st.address;
                st.mnemonic(2,"BA","",vec!(),|_| {});
                st.jump(Rvalue::Constant(next + 2),Guard::new());
                true
            });
        let sub2 = new_disassembler!(u8 =>
            [ 8 ] = |_| false);

        let main = new_disassembler!(u8 =>
            [ 1, sub ] = |_| true,
            [ 1 ] = |st: &mut State<u8>| {
                let next = st.address;
                st.mnemonic(1,"A","",vec!(),|_| {});
                st.jump(Rvalue::Constant(next + 1),Guard::new());
                true
            },
            [ "0 k@..... 11" ] = |st: &mut State<u8>| {
                let next = st.address;
                st.mnemonic(1,"C","",vec!(),|_| {});
                st.jump(Rvalue::Constant(next + 1),Guard::new());
                true
            },
            _ = |st: &mut State<u8>| {
                let next = st.address;
                st.mnemonic(1,"UNK","",vec!(),|_| {});
                st.jump(Rvalue::Constant(next + 1),Guard::new());
                true
            }
		);

        (sub,sub2,main,OpaqueLayer::wrap(vec!(1,1,2,1,3,8,1,8)))
	}

    #[test]
    fn single_decoder() {
        let (_,_,main,def) = fixture();
        let st = State::<u8>::new(0);
        let maybe_res = main.next_match(&mut def.iter(),st);

        assert!(maybe_res.is_some());
        let res = maybe_res.unwrap();

        assert_eq!(res.address, 0);
        assert_eq!(res.tokens.len(), 1);
        assert_eq!(res.tokens[0], 1);
        assert_eq!(res.groups.len(), 0);
        assert_eq!(res.mnemonics.len(), 1);
        assert_eq!(res.mnemonics[0].opcode, "A".to_string());
        assert_eq!(res.mnemonics[0].area, Bound::new(0,1));
        assert_eq!(res.mnemonics[0].instructions.len(), 0);
        assert_eq!(res.jumps.len(), 1);

        if let &(Rvalue::Constant(1),ref g) = &res.jumps[0] {
            assert_eq!(g, &Guard::new());
        } else {
            assert!(false);
        }
    }

    #[test]
    fn sub_decoder() {
        let (_,_,main,def) = fixture();
        let st = State::<u8>::new(1);
        let maybe_res = main.next_match(&mut def.iter().cut(&(1..def.len())),st);

        assert!(maybe_res.is_some());
        let res = maybe_res.unwrap();

        assert_eq!(res.address, 1);
        assert_eq!(res.tokens.len(), 2);
        assert_eq!(res.tokens[0], 1);
        assert_eq!(res.tokens[1], 2);
        assert_eq!(res.groups.len(), 0);
        assert_eq!(res.mnemonics.len(), 1);
        assert_eq!(res.mnemonics[0].opcode, "BA".to_string());
        assert_eq!(res.mnemonics[0].area, Bound::new(1,3));
        assert_eq!(res.mnemonics[0].instructions.len(), 0);
        assert_eq!(res.jumps.len(), 1);

        if let &(Rvalue::Constant(3),ref g) = &res.jumps[0] {
            assert_eq!(g, &Guard::new());
        } else {
            assert!(false);
        }
    }

    #[test]
    fn semantic_false() {
        let (_,sub2,_,def) = fixture();
        let st = State::<u8>::new(7);
        let maybe_res = sub2.next_match(&mut def.iter().cut(&(7..def.len())),st);

        assert!(maybe_res.is_none());
    }

    #[test]
    fn default_pattern() {
        let (_,_,main,def) = fixture();
        let st = State::<u8>::new(7);
        let maybe_res = main.next_match(&mut def.iter().cut(&(7..def.len())),st);

        assert!(maybe_res.is_some());
        let res = maybe_res.unwrap();

        assert_eq!(res.address, 7);
        assert_eq!(res.tokens.len(), 1);
        assert_eq!(res.tokens[0], 8);
        assert_eq!(res.groups.len(), 0);
        assert_eq!(res.mnemonics.len(), 1);
        assert_eq!(res.mnemonics[0].opcode, "UNK".to_string());
        assert_eq!(res.mnemonics[0].area, Bound::new(7,8));
        assert_eq!(res.mnemonics[0].instructions.len(), 0);
        assert_eq!(res.jumps.len(), 1);

        if let &(Rvalue::Constant(8),ref g) = &res.jumps[0] {
            assert_eq!(g, &Guard::new());
        } else {
            assert!(false);
        }
    }

    #[test]
    fn slice() {
        let (_,_,main,def) = fixture();
        let st = State::<u8>::new(1);
        let maybe_res = main.next_match(&mut def.iter().cut(&(1..2)),st);

        assert!(maybe_res.is_some());
        let res = maybe_res.unwrap();

        assert_eq!(res.address, 1);
        assert_eq!(res.tokens.len(), 1);
        assert_eq!(res.tokens[0], 1);
        assert_eq!(res.groups.len(), 0);
        assert_eq!(res.mnemonics.len(), 1);
        assert_eq!(res.mnemonics[0].opcode, "A".to_string());
        assert_eq!(res.mnemonics[0].area, Bound::new(1,2));
        assert_eq!(res.mnemonics[0].instructions.len(), 0);
        assert_eq!(res.jumps.len(), 1);

        if let &(Rvalue::Constant(2),ref g) = &res.jumps[0] {
            assert_eq!(g, &Guard::new());
        } else {
            assert!(false);
        }
     }

    #[test]
    fn empty() {
        let (_,_,main,def) = fixture();
        let st = State::<u8>::new(0);
        let maybe_res = main.next_match(&mut def.iter().cut(&(0..0)),st);

        assert!(maybe_res.is_none());
    }

    #[test]
    fn capture_group() {
        let (_,_,main,def) = fixture();
        let st = State::<u8>::new(4);
        let maybe_res = main.next_match(&mut def.iter().cut(&(4..def.len())),st);

        assert!(maybe_res.is_some());
        let res = maybe_res.unwrap();

        assert_eq!(res.address, 4);
        assert_eq!(res.tokens.len(), 1);
        assert_eq!(res.tokens[0], 3);
        assert_eq!(res.groups.len(), 1);
        assert_eq!(res.groups, vec!(("k".to_string(),0)));
        assert_eq!(res.mnemonics.len(), 1);
        assert_eq!(res.mnemonics[0].opcode, "C".to_string());
        assert_eq!(res.mnemonics[0].area, Bound::new(4,5));
        assert_eq!(res.mnemonics[0].instructions.len(), 0);
        assert_eq!(res.jumps.len(), 1);

        if let &(Rvalue::Constant(5),ref g) = &res.jumps[0] {
            assert_eq!(g, &Guard::new());
        } else {
            assert!(false);
        }
    }

    #[test]
    fn empty_capture_group() {
        let st = State::<u8>::new(0);
        let def = OpaqueLayer::wrap(vec!(127));
        let dec = new_disassembler!(u8 =>
            ["01 a@.. 1 b@ c@..."] = |st: &mut State<u8>| {
                st.mnemonic(1, "1","",vec!(),|_| {});
                true
            }
        );
        let maybe_res = dec.next_match(&mut def.iter(),st);

        assert!(maybe_res.is_some());
        let res = maybe_res.unwrap();

        assert_eq!(res.address, 0);
        assert_eq!(res.tokens.len(), 1);
        assert_eq!(res.tokens[0], 127);
        assert!(res.groups == vec!(("a".to_string(),3),("c".to_string(),7)) || res.groups == vec!(("c".to_string(),7),("a".to_string(),3)));
        assert_eq!(res.mnemonics.len(), 1);
        assert_eq!(res.mnemonics[0].opcode, "1".to_string());
        assert_eq!(res.mnemonics[0].area, Bound::new(0,1));
        assert_eq!(res.mnemonics[0].instructions.len(), 0);
        assert_eq!(res.jumps.len(), 0);
    }

    #[test]
    #[should_panic]
    fn too_long_capture_group() {
        new_disassembler!(u8 => [ "k@........." ] = |_| { true });
    }

    #[test]
    #[should_panic]
    fn too_long_token_pattern() {
        new_disassembler!(u8 => [ "111111111" ] = |_| { true });
    }

    #[test]
    #[should_panic]
    fn too_short_token_pattern() {
        new_disassembler!(u8 => [ "1111111" ] = |_| { true });
    }

    #[test]
    #[should_panic]
    fn invalid_token_pattern() {
        new_disassembler!(u8 => [ "a111111" ] = |_| { true });
    }
/*
    using sw = po::sem_state<wtest_tag>&;

    TEST_F(disassembler,wide_token)
    {
        po::sem_state<wtest_tag> st(0,'a');
        std::vector<uint8_t> _buf = {0x22,0x11, 0x44,0x33, 0x44,0x55};
        po::slab buf(_buf.data(),_buf.size());
        po::disassembler<wtest_tag> dec;

        dec[0x1122] = [](sw s)
        {
            s.mnemonic(2,"A");
            s.jump(s.address + 2);
            return true;
        };

        dec[0x3344] = [](sw s)
        {
            s.mnemonic(2,"B");
            s.jump(s.address + 2);
            s.jump(s.address + 4);
            return true;
        };

        dec[0x5544] = [](sw s)
        {
            s.mnemonic(2, "C");
            return true;
        };

        boost::optional<std::pair<po::slab::iterator,po::sem_state<wtest_tag>>> i;

        i = dec.try_match(buf.begin(),buf.end(),st);
        ASSERT_TRUE(!!i);
        st = i->second;

        ASSERT_EQ(std::distance(buf.begin(), i->first),2);
        ASSERT_EQ(st.address, 0u);
        ASSERT_EQ(st.tokens.size(), 1u);
        ASSERT_EQ(st.tokens[0], 0x1122u);
        ASSERT_EQ(st.mnemonics.size(), 1u);
        ASSERT_EQ(st.mnemonics.front().opcode, std::string("A"));
        ASSERT_EQ(st.mnemonics.front().area, po::bound(0,2));
        ASSERT_TRUE(st.mnemonics.front().instructions.empty());
        ASSERT_EQ(st.jumps.size(), 1u);
    }

    TEST_F(disassembler,optional)
    {

        po::sem_state<test_tag> st(0,'a');
        std::vector<unsigned char> _buf = {127,126,125,127,125};
        po::slab buf(_buf.data(),_buf.size());
        po::disassembler<test_tag> dec;

        dec[po::token_expr(127) >> *po::token_expr(126) >> po::token_expr(125)] = [](ss s) { s.mnemonic(s.tokens.size(), "1"); return true; };
        boost::optional<std::pair<po::slab::iterator,po::sem_state<test_tag>>> i;

        i = dec.try_match(buf.begin(),buf.end(),st);
        ASSERT_TRUE(!!i);
        st = i->second;

        ASSERT_EQ(std::distance(buf.begin(), i->first),3);
        ASSERT_EQ(st.address, 0u);
        ASSERT_EQ(st.tokens.size(), 3u);
        ASSERT_EQ(st.tokens[0], 127u);
        ASSERT_EQ(st.tokens[1], 126u);
        ASSERT_EQ(st.tokens[2], 125u);
        ASSERT_EQ(st.capture_groups.size(), 0u);
        ASSERT_EQ(st.mnemonics.size(), 1u);
        ASSERT_EQ(st.mnemonics.front().opcode, std::string("1"));
        ASSERT_EQ(st.mnemonics.front().area, po::bound(0,3));
        ASSERT_TRUE(st.mnemonics.front().instructions.empty());
        ASSERT_EQ(st.jumps.size(), 0u);

        st = po::sem_state<test_tag>(3,'a');
        i = dec.try_match(i->first,buf.end(),st);
        ASSERT_TRUE(!!i);
        st = i->second;

        ASSERT_EQ(std::distance(buf.begin(), i->first),5);
        ASSERT_EQ(st.address, 3u);
        ASSERT_EQ(st.tokens.size(), 2u);
        ASSERT_EQ(st.tokens[0], 127u);
        ASSERT_EQ(st.tokens[1], 125u);
        ASSERT_EQ(st.capture_groups.size(), 0u);
        ASSERT_EQ(st.mnemonics.size(), 1u);
        ASSERT_EQ(st.mnemonics.front().opcode, std::string("1"));
        ASSERT_EQ(st.mnemonics.front().area, po::bound(3,5));
        ASSERT_TRUE(st.mnemonics.front().instructions.empty());
        ASSERT_EQ(st.jumps.size(), 0u);
    }

    TEST_F(disassembler,fixed_capture_group_contents)
    {

        po::sem_state<test_tag> st(0,'a');
        std::vector<unsigned char> _buf = {127,255};
        po::slab buf(_buf.data(),_buf.size());
        po::disassembler<test_tag> dec;

        dec[ po::token_expr(std::string("01111111")) >> po::token_expr(std::string("a@11111111")) ] = [](ss s) { s.mnemonic(1,"1"); return true; };
        boost::optional<std::pair<po::slab::iterator,po::sem_state<test_tag>>> i;

        i = dec.try_match(buf.begin(),buf.end(),st);
        ASSERT_TRUE(!!i);
        st = i->second;

        ASSERT_EQ(std::distance(buf.begin(), i->first),2);
        ASSERT_EQ(st.address, 0u);
        ASSERT_EQ(st.tokens.size(), 2u);
        ASSERT_EQ(st.tokens[0], 127u);
        ASSERT_EQ(st.tokens[1], 255u);
        ASSERT_EQ(st.capture_groups.size(), 1u);
        ASSERT_EQ(st.capture_groups.count("a"), 1u);
        ASSERT_EQ(st.capture_groups["a"], 255u);
        ASSERT_EQ(st.mnemonics.size(), 1u);
        ASSERT_EQ(st.mnemonics.front().opcode, std::string("1"));
        ASSERT_EQ(st.mnemonics.front().area, po::bound(0,1));
        ASSERT_TRUE(st.mnemonics.front().instructions.empty());
        ASSERT_EQ(st.jumps.size(), 0u);
    }
*/
}
