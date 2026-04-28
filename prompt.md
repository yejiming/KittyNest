优化Tasks：
1. Task页面：去掉Create Task卡片
2. Agent Drawer：左侧菜单栏删掉最下方Local Ledger、SQLite synced这个卡片，改为一个Assistant按钮，点击后页面右侧弹出紧贴边框、悬浮在页面上方的Agent聊天Drawer；该Drawer左侧边框可用于调整宽度（设置一个合理的最小宽度）
3. Drawer功能：参考/Users/kc/Desktop/个人资料/个人项目/KittyCopilot项目（该项目使用python，本项目需要用rust实现），支持Agent助手功能，llm使用Settings中设置的Task模型
4. Tool列表和User Permission：参考/Users/kc/Desktop/个人资料/个人项目/KittyCopilot/kittycopilot/tools和/Users/kc/Desktop/个人资料/个人项目/KittyCopilot/kittycopilot/hooks/user_permission.py的实现（该项目使用python，本项目需要用rust实现），需支持read_file, ask_user, todo_write, grep, glob几个tool
5. 消息展示，参考KittyCopilot项目，消息基于websocket做流式展示，模型返回的<think></think>块内容放在Thinking块中（默认一行缩略，点击展开详情）；tool call和tool result放在Tool卡片（同一个tool call ID放一张卡片，默认一行缩略，点击展开详情）；Assistant消息在聊天栏展开全文（渲染成Markdown）；user_permission和ask_user在聊天栏内用卡片展示，用户选完后卡片隐藏；TODO项卡片放在输入框上沿中间，有TODO项时默认展开，可点击上沿中间的Font缩略/展开，无TODO项时该卡片隐藏
6. 输入框：Drawer下方为输入框，输入框左下角为圆形齿轮按钮，右下方为Send/Stop按钮（圆形Font表示），Send/Stop按钮左边为context小圆圈
7. 齿轮按钮：点击后弹出任务选择Modal，上方是任务类型标签，目前仅支持Task Assistant；进入该标签展示任务选项，Task Assistant需要选择Project（下拉框，仅支持状态为reviewed的Projects）
8. Send/Stop按钮：未发送信息展示为Send状态，发送后展示为Stop状态，Stop状态时点击可中断任务
9. context小圆圈：size比Send/Stop按钮小一号，用灰色圆环图展示agent使用上下文的占比，鼠标hover上去展开一个tooltip（风格适配整体UI，内容是当前context长度、System/User/Assistant/Thinking/Tool分别的上下文占比，一位小数百分比表示）
