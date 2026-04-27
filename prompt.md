1. 修改Project Summary和Progress内容：不将llm输出的<think></think>块中的内容放到保存的文件中
2. 修改Project详情页：第一张卡片需要展示3个Path：
    - 目前这个是Project Path
    - 还需要展示Project Summary Path（MD文件）和Project Progress Path（MD文件）
3. 为什么会有某个Task关联Session数为0的情况，例如这个task：LLM 并发上限调整为 5


1. 修改Task逻辑：
    - 调整summary.md更新逻辑：新的session被归入某个任务后，更新逻辑改为增量插入新的内容
    - summary.md保存路径改为：/Users/kc/.kittynest/projects/<project_name>/tasks/<task_slug>/
    - 调整Task详情页展示：最上方标题不变，下面第一张卡片展示Path和状态信息，Path为任务保存的路径，Status维持现有的discussing/developing/done三个选项，并且在右侧增加一个delete按钮，点击后删除该Task（只有归属Session数为0的Task可以删除）
    - 归属Session数为0的Task只允许为discussing状态
    - 下面依次用不同卡片展示Session给出的Summary，按更新时间顺序展示，每张卡片标明更新时间和所属Session（点击可以跳转到该Session详情页）
    - Related Sessions卡片删除
    - done状态的Task不能再往里新增归属的Session，如果原有Session往Task更新了新的Summary，将状态改为developing
2. 修改Session逻辑：
    - Session分析结果改为放在/Users/kc/.kittynest/projects/<project_name>/sessions/<session_slug>/
    - llm分析完session后，在对应的task summary.md增量更新一条信息，格式为{"content": "llm给出的summary", "timestamp": "session updated时间（不是当前时间）", "session": "session-slug"}
3. 用户在settings页点击Reset Tasks后，删除所有Task的文件夹
4. 用户在settings页点击Reset Sessions后，删除所有Session的文件夹
5. 用户在settings页点击Reset Projects后，删除所有Project的summary.md和progress.md


1. 修改Session Detail页面，将第一张卡片与第二张Path卡片合并，标题为第一张卡片标题，内容包括：原始Path、总结后的系统内Path
2. 分析Session的顺序，改为按Updated倒序（越新的Session越先被分析）
3. 修改Task逻辑：
    - 去掉当前Session自动新增Task的逻辑，仅支持用户在Task列表页手动新增Task
    - 新增Task时，下拉框选择Project（只能选状态为reviewed的Project），并在输入框输入提示词；llm基于用户提示词、Project的Summary和Progress、Project，提供一版更符合项目现实的提示词
    - 用户提示词保存在/Users/kc/.kittynest/projects/<project_name>/tasks/<task_slug>/user_prompt.md；llm给的提示词保存在/Users/kc/.kittynest/projects/<project_name>/tasks/<task_slug>/llm_prompt.md
    - Task详情页内容：Task Info卡片下为User Prompt卡片展示user_prompt.md的内容（渲染为markdown）；再下方为llm_prompt.md的内容（渲染为markdown）；再下方为Task Summary（读取/Users/kc/.kittynest/projects/<project_name>/tasks/<task_slug>/summary.md，渲染为markdown）；再下方为Session列表（展示session name，点击可进入🇭相关session）
    - Task详情页在Delete左边新增一个Summary按钮，点击后llm基于当前同Project下所有Session的summary，得到Task Summary（保存在/Users/kc/.kittynest/projects/<project_name>/tasks/<task_slug>/summary.md）和相关Session列表（保存在sqlite），注意不要有任何本地fallback逻辑，一定要使用llm得到结果
4. 修改Session逻辑：不再有更新task summary的逻辑
5. 现在各个卡片中渲染为markdown的地方，表格好像没有渲染成功，需要优化


1. 优化Dashboard页UI：
    - Projects卡片和Recent Sessions卡片的滑动条风格与整体不搭，并且默认状态不要展示出来
    - Projects/Recent Sessions的第一列名称只展示前10个字母，剩余用...表示
2. 目前的markdown渲染，是项目内实现的吗，可以考虑用外部包来渲染啊


目前项目还没有实现memory模块，我想要这样实现memory功能，帮我想想该怎么实现，先不深入技术细节，只讨论大致路线：
- llm取Session的summary信息，将Session级记忆写入 `~/.kittynest/memories/projects/<session_slug>/<session_slug>.md`
- llm一次性读取项目下所有的session级记忆，将项目级记忆写入 `~/.kittynest/memories/projects/<project_name>.md`
- llm一次性读取所有项目的项目级记忆，将系统级记忆写入 `~/.kittynest/memories/memory.md`
- 构建一个实体-关系组成的图数据库，将所有的session记忆关联起来