import { connect } from 'react-redux';
import SessionForm from './session_form';
import { login, CLEAR_SESSION_ERRORS } from '../../actions/session_actions';
import React from 'react'
import { Link } from 'react-router-dom'

const mapStateToProps = ({ errors }) => ({
  errors: Object.values(errors.session),
  formType: 'login'
})

const mapDispatchToProps = (dispatch) => ({
  processForm: (user) => (dispatch(login(user))),
  processDemoForm: user => dispatch(login(user)),
  clearErrors: () => dispatch({type: CLEAR_SESSION_ERRORS})
})

export default connect(mapStateToProps, mapDispatchToProps)(SessionForm)