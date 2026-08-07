import type { Metadata } from "next";
import Link from "next/link";
import { Brand } from "../components/Brand";
import { CARD_TYPE_NAMES } from "../lib/card-types";

export const metadata: Metadata = { title: "玩法规则", description: "openGuandan 使用的核心掼蛋规则与牌型说明。" };

const structures = [
  "任意一张牌",
  "两张牌点相同的牌",
  "三张牌点相同的牌",
  "一个三同张加一个对子",
  "五张牌点连续的单牌",
  "三个牌点连续的对子，共六张牌",
  "两个牌点连续的三同张，共六张牌",
  "四张或更多牌点相同的牌",
  "同一花色的五张顺子",
  "两张大王和两张小王",
];

export default function RulesPage() {
  return (
    <main className="rules-shell">
      <header className="site-header"><Brand /><nav><Link prefetch={false} href="/">返回首页</Link><Link prefetch={false} className="header-pill" href="/#start">开始游戏</Link></nav></header>
      <section className="rules-hero"><p className="eyebrow"><span /> openGuandan 规则</p><h1>先懂牌，再懂搭档。</h1><p>四人分为两队，相对而坐的玩家互为搭档。每位玩家获得 27 张牌，目标是在每轮牌中与搭档配合，尽快出完手牌。</p><div className="rules-jump"><a href="#cards">牌型</a><a href="#compare">大小比较</a><a href="#play">出牌</a><a href="#tribute">贡还牌</a><a href="#level">升级</a></div></section>

      <div className="rules-content">
        <aside><strong>快速索引</strong><a href="#basics">基本目标</a><a href="#cards">十种牌型</a><a href="#compare">牌型比较</a><a href="#play">出牌与借风</a><a href="#level">名次与升级</a><a href="#tribute">贡牌与还牌</a></aside>
        <article>
          <section id="basics"><span className="section-number">01</span><div><h2>玩家、牌张与目标</h2><p>使用两副标准扑克牌，共 108 张。两队级数都从 2 开始，依次打到 A；达到 A 后还要成功“过 A”才能赢得整局。</p><div className="rule-callout"><b>牌点从大到小</b><code>大王 ＞ 小王 ＞ 级牌 ＞ A ＞ K … ＞ 3 ＞ 2</code><p>当前级数对应点数的牌为级牌。两张红桃级牌称为“逢人配”，可以替代大小王以外的牌参与组合。</p></div></div></section>

          <section id="cards"><span className="section-number">02</span><div><h2>十种牌型</h2><div className="rules-table-wrap"><table><thead><tr><th>中文</th><th>English</th><th>构成</th></tr></thead><tbody>{Object.entries(CARD_TYPE_NAMES).map(([kind, names], index) => <tr key={kind}><td><b>{names.zh}</b></td><td>{names.en}</td><td>{structures[index]}</td></tr>)}</tbody></table></div><p className="rule-note">连续牌型中，A 可以作最高牌，也可作最低牌 1；不能循环连接。大小王不能进入连续牌型。</p></div></section>

          <section id="compare"><span className="section-number">03</span><div><h2>牌型比较</h2><p>普通牌型只能由相同牌型、相同张数且点数更大的牌压制。三带二只比较三同张部分；顺子、三连对和钢板比较连续组合的最高牌点。</p><div className="bomb-order"><span>四张炸弹</span><i>＜</i><span>五张炸弹</span><i>＜</i><span>同花顺</span><i>＜</i><span>六张及以上炸弹</span><i>＜</i><strong>天王炸</strong></div><p>炸弹类牌型可以压制任何普通牌型。普通炸弹先比较张数，张数相同再比较点数。</p></div></section>

          <section id="play"><span className="section-number">04</span><div><h2>出牌流程与借风</h2><ol><li>玩家按逆时针方向依次行动，领出者可以打出任意合法牌型。</li><li>后续玩家可出更大的合法牌型，也可以不出；不出不会永久退出本圈。</li><li>其他仍有手牌的玩家依次不出后，本圈结束，最后成功出牌者领出下一圈。</li><li>玩家必须一次性打出一手牌，不能分批补牌。</li></ol><div className="rule-callout"><b>借风</b><p>玩家的最后一手牌无人压制时，若其搭档仍有手牌，则由搭档领出下一圈。如果最后一手牌被压制，就由最后成功压制者领出。</p></div></div></section>

          <section id="level"><span className="section-number">05</span><div><h2>出完顺序与升级</h2><p>四人的名次依次为上游、二游、三游、下游。上游所在队赢得本轮牌，并按照搭档名次升级：</p><div className="level-grid"><div><span>上游 + 二游</span><b>升 3 级</b></div><div><span>上游 + 三游</span><b>升 2 级</b></div><div><span>上游 + 下游</span><b>升 1 级</b></div></div><p>同队包揽上游和二游称为“双下”。打 A 时，上游的搭档不能成为下游，才算成功过 A。</p></div></section>

          <section id="tribute"><span className="section-number">06</span><div><h2>贡牌与还牌</h2><h3>单贡</h3><p>上一轮下游向上游进贡手中点数最大的合资格牌；红桃级牌不能用于进贡。上游还一张不超过 10 的牌，随后由进贡者领出。</p><h3>双贡</h3><p>双下方两人各贡最大牌。上游取得较大的贡牌，其搭档取得较小的贡牌，并分别还牌。</p><h3>抗贡</h3><p>单贡者持两张大王，或双下方合计持两张大王时抗贡，不交换牌，由上一轮上游领出。</p></div></section>
        </article>
      </div>
      <section className="rules-cta"><p className="eyebrow"><span /> 规则看完了</p><h2>找三位朋友，开一桌。</h2><Link prefetch={false} className="button button--gold" href="/#start">开始游戏 <span>→</span></Link></section>
      <footer className="home-footer"><Brand compact /><p>本页为 openGuandan 核心规则摘要。</p><Link prefetch={false} href="/">返回首页 →</Link></footer>
    </main>
  );
}
