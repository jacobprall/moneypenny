import { connect } from 'react-redux';
import SessionForm from './session_form';
import { signUp } from '../../actions/session_actions';
import React from 'react'
import { Link } from 'react-router-dom'

const mapStateToProps = ({ errors }) => ({
  errors: errors.session,
  formType: 'signup'
})

const mapDispatchToProps = (dispatch) => ({
  processForm: (user) => (dispatch(signUp(user)))
})
// 

export default connect(mapStateToProps, mapDispatchToProps)(SessionForm)