优化Tasks：
1. Task页面：去掉Create Task卡片
2. Agent Drawer：左侧菜单栏删掉最下方Local Ledger、SQLite synced这个卡片，改为一个Assistant按钮，点击后页面右侧弹出紧贴边框、悬浮在页面上方的Agent聊天Drawer；该Drawer左侧边框可用于调整宽度（设置一个合理的最小宽度）
3. Drawer功能：参考/Users/kc/Desktop/个人资料/个人项目/KittyCopilot项目（该项目使用python，本项目需要用rust实现），支持Agent助手功能，llm使用Settings中设置的Assistant模型
4. Tool列表和User Permission：参考/Users/kc/Desktop/个人资料/个人项目/KittyCopilot/kittycopilot/tools和/Users/kc/Desktop/个人资料/个人项目/KittyCopilot/kittycopilot/hooks/user_permission.py的实现（该项目使用python，本项目需要用rust实现），需支持read_file, ask_user, todo_write, grep, glob几个tool
5. 消息展示，参考KittyCopilot项目，消息基于websocket做流式展示，模型返回的<think></think>块内容放在Thinking块中（默认一行缩略，点击展开详情）；tool call和tool result放在Tool卡片（同一个tool call ID放一张卡片，默认一行缩略，点击展开详情）；Assistant消息在聊天栏展开全文（渲染成Markdown）；user_permission和ask_user在聊天栏内用卡片展示，用户选完后卡片隐藏；TODO项卡片放在输入框上沿中间，有TODO项时默认展开，可点击上沿中间的Font缩略/展开，无TODO项时该卡片隐藏
6. 输入框：Drawer下方为输入框，输入框左下角为圆形齿轮按钮，右下方为Send/Stop按钮（圆形Font表示），Send/Stop按钮左边为context小圆圈
7. 齿轮按钮：点击后弹出任务选择Modal，上方是任务类型标签，目前仅支持Task Assistant；进入该标签展示任务选项，Task Assistant需要选择Project（下拉框，仅支持状态为reviewed的Projects）
8. Send/Stop按钮：未发送信息展示为Send状态，发送后展示为Stop状态，Stop状态时点击可中断任务
9. context小圆圈：size比Send/Stop按钮小一号，用灰色圆环图展示agent使用上下文的占比，鼠标hover上去展开一个tooltip（风格适配整体UI，内容是当前context长度、System/User/Assistant/Thinking/Tool分别的上下文占比，一位小数百分比表示）

1. Assistant Drawer需要修改
- 所有Assistant相关代码文件，整理到src-tauri/src/assistant文件夹
- 现在中途Stop，再发信息，会导致HTTP status client error (400 Bad Request) for url，我怀疑是正在进行中的tool call，没有tool result返回，导致出现context不合法
- 现在Thinking块和Tool块，完成后直接隐藏了，不能在聊天栏可见；需要改成用户可以在聊天栏展开/缩略Thinking块和Tool块
- 隐藏drawer的垂直滑动条展示
2. 启动速度优化：
- 现在桌面应用启动时，前端会立即触发扫描claude code和codex的session目录，去掉这个步骤



1. 增加Drawer Refresh功能：
    - Drawer右上角关闭按钮左边增加Refresh按钮（圆形Font表示），点击后清空当前Session上下文
    - 在Drawer设置中切换Project、在Settings中修改Assistant Model，都会自动触发Refresh
2. 增加Drawer Save功能：
    - 在Refresh按钮左边增加Save按钮（圆形Font表示），点击后保存该Session，对于当前仅有的Task Assistant型任务，session保存在/Users/kc/.kittynest/projects/<project_name>/tasks/<task_slug>
    - 保存时调用Assistant Model，输入该session的user_message和assistant_message，得到Task Name + Task Description
    - 保存后，在Tasks列表页可以看到该Session，Tasks列表字段改为Name、Project、Status、Created，刚创建的Task默认Status=Discussing，用户可修改Status（Discussing/Developing/Done）
    - 点击Task元素，进入Task详情页，第一张卡片展示Task Name和Project、Status，Created等信息，下方展示Task Description（markdown渲染+隐藏的垂直滑动条），再下方卡片展示对话记录（仅User Message和Assistant Message，UI与Drawer聊天栏类似，只是宽度不一样）；页面右上角有Delete按钮和Load按钮，点击Delete删除该Task，点击Load在Drawer聊天栏加载该Session（完整加载所有上下文，渲染Thinking块、Tool块、User Message、Assistant Message等）
3. 增加read_memory tool：使用entity在数据库查询关联memory
4. 增加create_task tool：基于当前agent上下文，生成Task Name + Task Description，需符合用户准确意图、符合项目实际进展和用户历史偏好，在页面中间弹出一个Modal（页面中间非Drawer中间），正文处是任务标题+任务描述，带垂直滑动条（隐藏显示），下方有Accept/Cancel按钮，点击Accept后创建Task，刚创建的Task默认Status=Discussing
