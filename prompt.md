1. 修改Session Detail页面，将第一张卡片与第二张Path卡片合并，标题为第一张卡片标题，内容包括：原始Path、总结后的系统内Path
2. 分析Session的顺序，改为按Updated倒序（越新的Session越先被分析）
3. 修改Task逻辑：
    - 去掉当前Session自动新增Task的逻辑，仅支持用户在Task列表页手动新增Task
    - 新增Task时，下拉框选择Project（只能选状态为reviewed的Project），并在输入框输入提示词；llm基于用户提示词、Project的Summary和Progress、Project，提供一版更符合项目现实的提示词
    - 用户提示词保存在/Users/kc/.kittynest/projects/<project_name>/tasks/<task_slug>/user_prompt.md；llm给的提示词保存在/Users/kc/.kittynest/projects/<project_name>/tasks/<task_slug>/llm_prompt.md
    - Task详情页内容：Task Info卡片下为User Prompt卡片展示user_prompt.md的内容（渲染为markdown）；再下方为llm_prompt.md的内容（渲染为markdown）；再下方为Task Summary（读取/Users/kc/.kittynest/projects/<project_name>/tasks/<task_slug>/summary.md，渲染为markdown）；再下方为Session列表（展示session name，点击可进入相关session）
    - Task详情页在Delete左边新增一个Summary按钮，点击后llm基于当前同Project下所有Session的summary，得到Task Summary（保存在/Users/kc/.kittynest/projects/<project_name>/tasks/<task_slug>/summary.md）和相关Session列表（保存在sqlite），注意不要有任何本地fallback逻辑，一定要使用llm得到结果
4. 修改Session逻辑：不再有更新task summary的逻辑
5. 现在各个卡片中渲染为markdown的地方，表格好像没有渲染成功，需要优化


优化Settings：
1. LLM Global Settings修改后没法保存，卡片左上角增加一个Save按钮
2. config.toml中[[llm]]api_key, base_url好像是冗余字段，可以删掉，不需要兼容旧逻辑
3. LLM Provider左栏可以宽一些，去掉Saved Models和Use Model For这两个标题，下面每一个下拉框，标题和下拉框放同一行
4. 左栏的模型列表下方增加一个加号虚框，点击后右栏展示一个空白设置项，输入内容点击Save Model后，在模型列表中增加一个模型
5. 左栏高度与右栏保持一致，如果内容超出，则增加垂直滑动条，UI上隐藏该滑动条
6. 左栏的Task Model改名为Assistant Model，支持Assistant Drawer任务（当前留空）
7. Memory模型不仅要用于entity消歧，还要用于memory搜索时对用户输入的entity提取

优化memory搜索：在 extract_memory_search_entities 调用前，先把 graph 中已有的 entity 列表取出来，让 LLM 从中挑选出现在
查询里的 entity，而不是自由提取：
Graph entities: ["kittycopilot", "tauri", "react", "sqlite", ...]
User query: "kittycopilot项目是做什么的"
Return JSON: {"entities": ["kittycopilot"]}
这样 LLM 不会发明 "kittycopilot项目" 这种 graph 中不存在的变体。

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



    