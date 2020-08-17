export const RECEIVE_CURRENT_USER = "RECEIVE_CURRENT_USER"
export const RECEIVE_ERRORS = "RECEIVE_ERRORS"
export const LOGOUT_CURRENT_USER = "LOGOUT_CURRENT_USER"
import * as SessionUtil from '../util/session_api_util'


export const receiveCurrentUser = (user) => ({
  type: RECEIVE_CURRENT_USER,
  user
})

export const receiveErrors = (errors) => ({
  type: RECEIVE_ERRORS,
  errors
})

export const logoutCurrentUser = () => ({
  type: LOGOUT_CURRENT_USER
})

//Thunk Action Creators

export const login = (user) => dispatch => (
  SessionUtil.signIn(user).then(user => dispatch(receiveCurrentUser(user)))
)

export const logout = () => dispatch => {
  return SessionUtil.signOut().then(() => dispatch(logoutCurrentUser()))
}

export const signUp = (user) => dispatch => (
  SessionUtil.signUp(user).then(user => dispatch(receiveCurrentUser(user)))
)
