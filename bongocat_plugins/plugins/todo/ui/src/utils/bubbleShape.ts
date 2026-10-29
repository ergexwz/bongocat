/**
 * 手绘气泡 SVG 形状生成（T2）。
 *
 * 根因（用户报告的 bug）：设计稿原 path 的坐标是写死的（viewBox 360×100，底部边框 y≈66），
 * 当内容（文字换行）变高时，内容会溢出到形状外面——底部边框不会跟随内容下移。
 *
 * 本函数把「纯文字高度」+「宽度」作为输入，动态生成 path 的 d 属性 + viewBox，让形状的
 * 底部边框和尾部三角始终跟随内容底部移动、宽度跟随内容变宽。上下留白对称（都是 PAD），
 * 文字夹在中间。
 *
 * 模型（viewBox 坐标 = CSS 像素，1:1 不拉伸）：
 *   - 顶部边框 y = TOP（固定 10）
 *   - 文字区 [TOP+PAD, TOP+PAD+textH]（上下留白都是 PAD，对称）
 *   - 底部边框 y = TOP + PAD + textH + PAD
 *   - 尾部尖角 y = 底部边框 + TAIL（在底部边框下方凸出，指向桌宠），水平居中 W/2
 *   - viewBox 高 = 尾尖 + VB_BOT（给尖角底部留余量）
 *
 * 宽度自适应：默认 360（对齐 todo 面板）；内容更宽（如长链接不换行）时按实际宽度生成，
 * 避免内容溢出形状右侧。所有 x 坐标按 W 推导（右上角 W-20、右侧曲线 W-12、尾部 W/2）。
 *
 * 容器（.bubble）width/height 必须设为 viewBox 尺寸（1:1 不拉伸），否则
 * preserveAspectRatio="none" 会拉伸形状。
 *
 * 在真浏览器 harness 验证：短主题 + 长主题多行两种情况，边框完整包裹文字、上下留白对称、
 * 尾部居中不重叠。
 *
 * @see docs/designs/phase2-exploration/bubble.html 原始写死 path 的视觉来源
 */

/** 顶部边框 y（固定）。 */
export const SHAPE_TOP = 10
/** 文字上下留白（对称，与原设计 padding 16 一致）。 */
export const SHAPE_PAD = 16
/** 尾部三角高度（底部边框下方凸出）。 */
export const SHAPE_TAIL = 14
/** viewBox 底部余量（尾尖下方）。 */
export const SHAPE_VB_BOT = 6
/** 默认宽度（对齐 todo 面板，内容更窄时不缩）。 */
export const SHAPE_DEFAULT_WIDTH = 360

export interface BubbleShape {
  /** viewBox 字符串（"0 0 W H"）。 */
  viewBox: string
  /** viewBox 高度（容器 + SVG 的 height 都用此值，保证 1:1 不拉伸）。 */
  height: number
  /** 形状宽度（容器 + SVG 的 width 都用此值）。 */
  width: number
  /** 阴影层 path d。 */
  shadowD: string
  /** 主形状 path d（白底 + 描边）。 */
  mainD: string
  /** 顶部装饰波浪 path d。 */
  waveD: string
  /** 内容区 top（CSS 像素，content 元素绝对定位用）。 */
  contentTop: number
}

/**
 * 根据纯文字高度 + 宽度生成手绘气泡的 SVG path + viewBox + 尺寸。
 *
 * @param textHeight 纯文字高度（px，不含上下 padding；通常 = content 元素 getBoundingClientRect().height）
 * @param width      形状宽度（px），默认 360；长内容（如长链接）可传更大值避免溢出
 */
export function genBubbleShape(textHeight: number, width: number = SHAPE_DEFAULT_WIDTH): BubbleShape {
  const W = width
  const topEdge = SHAPE_TOP
  const contentTop = SHAPE_TOP + SHAPE_PAD
  const bottomEdge = contentTop + textHeight + SHAPE_PAD
  const tailTip = bottomEdge + SHAPE_TAIL
  const height = tailTip + SHAPE_VB_BOT
  // 尾部三角水平居中（指向桌宠头顶），底边半宽 10
  const tailCx = Math.round(W / 2)
  const tailHalf = 10

  return {
    viewBox: `0 0 ${W} ${height}`,
    height,
    width: W,
    contentTop,
    shadowD: `M 16 ${topEdge + 4} Q 18 ${topEdge} 24 ${topEdge + 1} L ${W - 18} ${topEdge + 4} Q ${W - 10} ${topEdge + 6} ${W - 11} ${topEdge + 12} L ${W - 10} ${bottomEdge + 4} Q ${W - 12} ${bottomEdge + 10} ${W - 18} ${bottomEdge + 10} L ${tailCx + tailHalf + 2} ${bottomEdge + 10} L ${tailCx + 2} ${tailTip + 4} L ${tailCx - tailHalf + 2} ${bottomEdge + 10} L 22 ${bottomEdge + 8} Q 14 ${bottomEdge + 6} 15 ${bottomEdge} Z`,
    mainD: `M 14 ${topEdge} Q 16 ${topEdge - 4} 22 ${topEdge - 3} L ${W - 20} ${topEdge} Q ${W - 12} ${topEdge + 2} ${W - 13} ${topEdge + 8} L ${W - 12} ${bottomEdge - 6} Q ${W - 14} ${bottomEdge} ${W - 20} ${bottomEdge} L ${tailCx + tailHalf} ${bottomEdge} L ${tailCx} ${tailTip} L ${tailCx - tailHalf} ${bottomEdge} L 20 ${bottomEdge - 2} Q 12 ${bottomEdge - 4} 13 ${bottomEdge - 10} Z`,
    waveD: `M 36 ${topEdge + 16} Q 46 ${topEdge + 12} 56 ${topEdge + 16} T 76 ${topEdge + 16}`,
  }
}
