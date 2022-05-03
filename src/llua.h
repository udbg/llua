
#ifndef __LLUA_H__
#define __LLUA_H__

#define lua_lock(L) llua_lock(L)
#define lua_unlock(L) llua_unlock(L)
#define luai_userstateopen(L) llua_userstateopen(L)
#define luai_userstateclose(L) llua_userstateclose(L)

extern void llua_lock(lua_State *L);
extern void llua_unlock(lua_State *L);
extern void llua_userstateopen(lua_State *L);
extern void llua_userstateclose(lua_State *L);

#endif /* __LLUA_H__ */
